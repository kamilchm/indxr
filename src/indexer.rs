use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use rayon::prelude::*;

use crate::cache::Cache;
use crate::model::{CodebaseIndex, FileIndex, IndexStats, MemberIndex, WorkspaceIndex};
use crate::output::OutputFormatter;
use crate::output::markdown::{MarkdownFormatter, MarkdownOptions};
use crate::parser::ParserRegistry;
use crate::walker::{self, FileEntry};
use crate::workspace::{self, Workspace};

/// Configuration needed to (re-)build an index.
#[derive(Clone, Debug)]
pub struct IndexConfig {
    pub root: PathBuf,
    pub cache_dir: PathBuf,
    pub max_file_size: u64,
    pub max_depth: Option<usize>,
    pub exclude: Vec<String>,
    pub no_gitignore: bool,
}

pub struct ParseResult {
    pub file_index: FileIndex,
    pub relative_path: PathBuf,
    pub size: u64,
    pub mtime: u64,
    pub content_bytes: Option<Vec<u8>>,
}

pub fn parse_files(
    files: &[&FileEntry],
    cache: &Cache,
    registry: &ParserRegistry,
) -> Vec<ParseResult> {
    files
        .par_iter()
        .filter_map(|file_entry| {
            let file_entry = *file_entry;

            // Check cache first
            if let Some(cached) =
                cache.get(&file_entry.relative_path, file_entry.size, file_entry.mtime)
            {
                return Some(ParseResult {
                    file_index: cached,
                    relative_path: file_entry.relative_path.clone(),
                    size: file_entry.size,
                    mtime: file_entry.mtime,
                    content_bytes: None,
                });
            }

            // Parse the file
            let parser = registry.get_parser(&file_entry.language)?;
            let content = fs::read_to_string(&file_entry.path).ok()?;
            let mut index = parser
                .parse_file(&file_entry.relative_path, &content)
                .ok()?;
            index.size = file_entry.size;

            Some(ParseResult {
                file_index: index,
                relative_path: file_entry.relative_path.clone(),
                size: file_entry.size,
                mtime: file_entry.mtime,
                content_bytes: Some(content.into_bytes()),
            })
        })
        .collect()
}

pub fn collect_results(
    results: Vec<ParseResult>,
    cache: &mut Cache,
) -> (Vec<FileIndex>, usize, HashMap<String, usize>, usize) {
    let mut file_indices = Vec::new();
    let mut total_lines = 0;
    let mut language_counts: HashMap<String, usize> = HashMap::new();
    let mut cache_hits = 0usize;

    for result in results {
        if let Some(ref bytes) = result.content_bytes {
            cache.insert(
                &result.relative_path,
                result.size,
                result.mtime,
                bytes,
                result.file_index.clone(),
            );
        } else {
            cache_hits += 1;
        }
        total_lines += result.file_index.lines;
        *language_counts
            .entry(result.file_index.language.name().to_string())
            .or_insert(0) += 1;
        file_indices.push(result.file_index);
    }

    (file_indices, total_lines, language_counts, cache_hits)
}

/// Build a fresh `CodebaseIndex` from the given configuration.
pub fn build_index(config: &IndexConfig) -> anyhow::Result<CodebaseIndex> {
    let root = fs::canonicalize(&config.root)?;
    let root_name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "project".to_string());

    let exclude_patterns = &config.exclude;
    let walk_result = walker::walk_directory(
        &root,
        !config.no_gitignore,
        config.max_file_size,
        config.max_depth,
        exclude_patterns,
    )?;

    let mut cache = Cache::load(&config.cache_dir);
    let registry = ParserRegistry::new();

    let file_refs: Vec<&FileEntry> = walk_result.files.iter().collect();
    let results = parse_files(&file_refs, &cache, &registry);
    let (mut file_indices, total_lines, language_counts, _) = collect_results(results, &mut cache);

    file_indices.sort_by(|a, b| a.path.cmp(&b.path));

    cache.prune(
        &walk_result
            .files
            .iter()
            .map(|f| f.relative_path.clone())
            .collect::<Vec<_>>(),
    );
    cache.save()?;

    Ok(CodebaseIndex {
        root,
        root_name,
        generated_at: chrono::Utc::now()
            .format("%Y-%m-%d %H:%M:%S UTC")
            .to_string(),
        files: file_indices,
        tree: walk_result.tree,
        stats: IndexStats {
            total_files: walk_result.files.len(),
            total_lines,
            languages: language_counts,
            duration_ms: 0,
        },
    })
}

// ---------------------------------------------------------------------------
// Workspace-level indexing
// ---------------------------------------------------------------------------

/// Configuration for workspace-level indexing.
#[derive(Clone, Debug)]
pub struct WorkspaceConfig {
    /// The detected workspace.
    pub workspace: Workspace,
    /// Template config (max_file_size, max_depth, exclude, no_gitignore).
    /// The `root` and `cache_dir` fields are overridden per-member.
    pub template: IndexConfig,
}

/// Build a `WorkspaceIndex` by indexing each member independently.
pub fn build_workspace_index(ws_config: &WorkspaceConfig) -> anyhow::Result<WorkspaceIndex> {
    let workspace = &ws_config.workspace;
    let start = std::time::Instant::now();

    let members: Vec<anyhow::Result<MemberIndex>> = workspace
        .members
        .par_iter()
        .map(|member| {
            let mut exclude = ws_config.template.exclude.clone();
            exclude.extend(member_specific_excludes(member, workspace));
            let member_config = IndexConfig {
                root: member.path.clone(),
                cache_dir: ws_config.template.cache_dir.join(&member.name),
                max_file_size: ws_config.template.max_file_size,
                max_depth: ws_config.template.max_depth,
                exclude,
                no_gitignore: ws_config.template.no_gitignore,
            };
            let mut index = build_index(&member_config)?;
            normalize_member_index_paths(&mut index, &member.relative_path);
            Ok(MemberIndex {
                name: member.name.clone(),
                relative_path: member.relative_path.clone(),
                index,
            })
        })
        .collect();

    let mut member_indices = Vec::new();
    for result in members {
        member_indices.push(result?);
    }

    let stats = aggregate_stats(&member_indices, start.elapsed());

    Ok(WorkspaceIndex {
        root: workspace.root.clone(),
        root_name: workspace
            .root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "workspace".to_string()),
        workspace_kind: workspace.kind,
        generated_at: chrono::Utc::now()
            .format("%Y-%m-%d %H:%M:%S UTC")
            .to_string(),
        members: member_indices,
        stats,
    })
}

/// Detect workspace and build a `WorkspaceIndex`.
/// If no workspace is detected, creates a single-member workspace wrapping `build_index`.
/// Returns both the index and the config so callers don't need to re-detect the workspace.
pub fn detect_and_build_workspace(
    root: &std::path::Path,
    config: &IndexConfig,
    no_workspace: bool,
    member_filter: Option<&[String]>,
) -> anyhow::Result<(WorkspaceIndex, WorkspaceConfig)> {
    let mut workspace = if no_workspace {
        workspace::single_root_workspace(&fs::canonicalize(root)?)
    } else {
        workspace::detect_workspace(root)?
    };

    // Filter to specific members if requested
    if let Some(filter) = member_filter {
        let filter_lower: Vec<String> = filter.iter().map(|s| s.to_lowercase()).collect();
        workspace
            .members
            .retain(|m| filter_lower.contains(&m.name.to_lowercase()));
        if workspace.members.is_empty() {
            anyhow::bail!(
                "No matching workspace members found for: {}",
                filter.join(", ")
            );
        }
    }

    let ws_config = WorkspaceConfig {
        workspace,
        template: config.clone(),
    };

    let ws_index = build_workspace_index(&ws_config)?;
    Ok((ws_index, ws_config))
}

fn member_specific_excludes(
    member: &workspace::WorkspaceMember,
    workspace: &Workspace,
) -> Vec<String> {
    let mut excludes = Vec::new();

    for other in &workspace.members {
        if other.path == member.path {
            continue;
        }

        if let Ok(relative) = other.path.strip_prefix(&member.path) {
            if relative.as_os_str().is_empty() {
                continue;
            }

            excludes.push(format!("{}/**", relative.to_string_lossy()));
        }
    }

    excludes.sort();
    excludes.dedup();
    excludes
}

fn normalize_member_index_paths(index: &mut CodebaseIndex, member_relative_path: &Path) {
    if member_relative_path.as_os_str().is_empty() || member_relative_path == Path::new(".") {
        return;
    }

    for file in &mut index.files {
        file.path = member_relative_path.join(&file.path);
    }
}

/// Rebuild a workspace index and write INDEX.md to the workspace root.
pub fn regenerate_workspace_index(ws_config: &WorkspaceConfig) -> anyhow::Result<WorkspaceIndex> {
    let ws_index = build_workspace_index(ws_config)?;
    let markdown = generate_workspace_markdown(&ws_index)?;
    let output_path = ws_index.root.join("INDEX.md");
    fs::write(&output_path, &markdown)?;
    Ok(ws_index)
}

/// Generate INDEX.md content from a `WorkspaceIndex`.
pub fn generate_workspace_markdown(ws_index: &WorkspaceIndex) -> anyhow::Result<String> {
    use crate::model::DetailLevel;
    use std::fmt::Write;

    let formatter = MarkdownFormatter::with_options(MarkdownOptions {
        omit_imports: false,
        omit_tree: false,
    });

    if ws_index.is_single() {
        // Single-member workspace: just format the inner index
        return formatter.format(&ws_index.members[0].index, DetailLevel::Signatures);
    }

    let mut out = String::new();
    writeln!(out, "# Workspace Index: {}", ws_index.root_name)?;
    writeln!(out)?;
    writeln!(
        out,
        "> Generated: {} | Workspace: {} | Members: {} | Files: {} | Lines: {}",
        ws_index.generated_at,
        ws_index.workspace_kind.as_str(),
        ws_index.members.len(),
        ws_index.stats.total_files,
        ws_index.stats.total_lines
    )?;
    writeln!(out)?;

    // Members table
    writeln!(out, "## Members")?;
    writeln!(out)?;
    writeln!(out, "| Member | Path | Files | Lines |")?;
    writeln!(out, "|--------|------|-------|-------|")?;
    for member in &ws_index.members {
        writeln!(
            out,
            "| {} | {} | {} | {} |",
            member.name,
            member.relative_path.display(),
            member.index.stats.total_files,
            member.index.stats.total_lines
        )?;
    }
    writeln!(out)?;

    // Per-member sections
    for member in &ws_index.members {
        writeln!(out, "---")?;
        writeln!(out)?;
        let member_md = formatter.format(&member.index, DetailLevel::Signatures)?;
        out.push_str(&member_md);
        writeln!(out)?;
    }

    Ok(out)
}

fn aggregate_stats(members: &[MemberIndex], duration: std::time::Duration) -> IndexStats {
    let mut total_files = 0;
    let mut total_lines = 0;
    let mut languages: HashMap<String, usize> = HashMap::new();

    for member in members {
        total_files += member.index.stats.total_files;
        total_lines += member.index.stats.total_lines;
        for (lang, count) in &member.index.stats.languages {
            *languages.entry(lang.clone()).or_insert(0) += count;
        }
    }

    IndexStats {
        total_files,
        total_lines,
        languages,
        duration_ms: duration.as_millis() as u64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_build_workspace_index_scopes_root_member_and_normalizes_paths() {
        let tmp = tempdir().unwrap();
        let root = tmp.path();

        fs::write(
            root.join("Cargo.toml"),
            r#"[package]
name = "rootpkg"
version = "0.1.0"
edition = "2021"

[workspace]
members = [".", "apps/api"]
"#,
        )
        .unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();

        fs::create_dir_all(root.join("apps/api/src")).unwrap();
        fs::write(
            root.join("apps/api/Cargo.toml"),
            r#"[package]
name = "api"
version = "0.1.0"
edition = "2021"
"#,
        )
        .unwrap();
        fs::write(root.join("apps/api/src/lib.rs"), "pub fn run() {}\n").unwrap();

        let workspace = workspace::detect_workspace(root).unwrap();
        let ws_config = WorkspaceConfig {
            workspace,
            template: IndexConfig {
                root: root.to_path_buf(),
                cache_dir: root.join(".indxr-cache-test"),
                max_file_size: 512,
                max_depth: None,
                exclude: Vec::new(),
                no_gitignore: true,
            },
        };

        let ws_index = build_workspace_index(&ws_config).unwrap();
        let root_member = ws_index.find_member("rootpkg").unwrap();
        let api_member = ws_index.find_member("api").unwrap();

        let root_paths: Vec<String> = root_member
            .index
            .files
            .iter()
            .map(|f| f.path.to_string_lossy().to_string())
            .collect();
        assert!(root_paths.contains(&"Cargo.toml".to_string()));
        assert!(root_paths.contains(&"src/main.rs".to_string()));
        assert!(!root_paths.contains(&"apps/api/src/lib.rs".to_string()));

        let api_paths: Vec<String> = api_member
            .index
            .files
            .iter()
            .map(|f| f.path.to_string_lossy().to_string())
            .collect();
        assert!(api_paths.contains(&"apps/api/Cargo.toml".to_string()));
        assert!(api_paths.contains(&"apps/api/src/lib.rs".to_string()));
    }
}
