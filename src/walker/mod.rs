use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::Result;
use ignore::WalkBuilder;

use crate::languages::Language;
use crate::model::TreeEntry;

pub struct WalkResult {
    pub files: Vec<FileEntry>,
    pub tree: Vec<TreeEntry>,
}

pub struct FileEntry {
    pub path: PathBuf,
    pub relative_path: PathBuf,
    pub language: Language,
    pub size: u64,
    pub mtime: u64,
}

pub fn walk_directory(
    root: &Path,
    respect_gitignore: bool,
    max_file_size: u64,
    max_depth: Option<usize>,
    exclude_patterns: &[String],
) -> Result<WalkResult> {
    let mut builder = WalkBuilder::new(root);
    builder.git_ignore(respect_gitignore);
    builder.hidden(true);

    if !exclude_patterns.is_empty() {
        let mut overrides = ignore::overrides::OverrideBuilder::new(root);
        for pattern in exclude_patterns {
            overrides.add(&format!("!{}", pattern))?;
        }
        builder.overrides(overrides.build()?);
    }

    if let Some(depth) = max_depth {
        builder.max_depth(Some(depth));
    }

    let mut files = Vec::new();
    let mut dirs: BTreeSet<PathBuf> = BTreeSet::new();

    for entry in builder.build() {
        let entry = entry?;
        let path = entry.path();
        let relative = path.strip_prefix(root).unwrap_or(path);

        // Skip non-files
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }

        let metadata = entry.metadata()?;
        let size = metadata.len();

        // Skip files over the size limit (max_file_size is in KB)
        if size > max_file_size * 1024 {
            continue;
        }

        let mtime = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        // Only include files with recognized languages
        if let Some(language) = Language::detect(path) {
            // Track ancestor directories
            let mut parent = relative.parent();
            while let Some(p) = parent {
                if !p.as_os_str().is_empty() {
                    dirs.insert(p.to_path_buf());
                }
                parent = p.parent();
            }

            files.push(FileEntry {
                path: path.to_path_buf(),
                relative_path: relative.to_path_buf(),
                language,
                size,
                mtime,
            });
        }
    }

    // Build tree entries sorted by path
    let mut tree = Vec::new();
    let mut all_paths: Vec<(PathBuf, bool)> = Vec::new();

    for dir in &dirs {
        all_paths.push((dir.clone(), true));
    }
    for file in &files {
        all_paths.push((file.relative_path.clone(), false));
    }
    all_paths.sort_by(|a, b| a.0.cmp(&b.0));

    for (path, is_dir) in all_paths {
        let depth = path.components().count();
        let display_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());

        tree.push(TreeEntry {
            path: display_name,
            is_dir,
            depth,
        });
    }

    // Sort files by relative path
    files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));

    Ok(WalkResult { files, tree })
}
