use std::path::Path;

use crate::model::CodebaseIndex;
use crate::model::declarations::{DeclKind, Declaration, Visibility};

pub struct FilterOptions {
    pub filter_path: Option<String>,
    pub symbol: Option<String>,
    pub kind: Option<DeclKind>,
    pub public_only: bool,
}

impl FilterOptions {
    pub fn is_active(&self) -> bool {
        self.filter_path.is_some()
            || self.symbol.is_some()
            || self.kind.is_some()
            || self.public_only
    }
}

/// Apply filters to a CodebaseIndex, returning a new filtered index.
/// Filters are applied in order: path, kind, public_only, symbol.
pub fn apply_filters(index: &mut CodebaseIndex, opts: &FilterOptions) {
    if !opts.is_active() {
        return;
    }

    // 1. Filter by path prefix
    if let Some(ref prefix) = opts.filter_path {
        let prefix_path = Path::new(prefix);
        index
            .files
            .retain(|file| file.path.starts_with(prefix_path));
        index.tree.retain(|entry| {
            let entry_path = Path::new(&entry.path);
            entry_path.starts_with(prefix_path) || prefix_path.starts_with(entry_path)
        });
    }

    // 2. Filter by declaration kind
    if let Some(ref kind) = opts.kind {
        for file in &mut index.files {
            file.declarations = filter_declarations_by_kind(&file.declarations, kind);
        }
        // Remove files that have no declarations left after kind filtering
        index.files.retain(|file| !file.declarations.is_empty());
    }

    // 3. Filter by public_only (remove private declarations)
    if opts.public_only {
        for file in &mut index.files {
            file.declarations = filter_declarations_by_visibility(&file.declarations);
        }
        index.files.retain(|file| !file.declarations.is_empty());
    }

    // 4. Filter by symbol name (case-insensitive substring match)
    if let Some(ref symbol) = opts.symbol {
        let query = symbol.to_lowercase();
        for file in &mut index.files {
            file.declarations = filter_declarations_by_symbol(&file.declarations, &query);
        }
        index.files.retain(|file| !file.declarations.is_empty());
    }

    // Recalculate stats
    recalculate_stats(index);
}

/// Recursively filter declarations, keeping only those matching the given kind.
/// A parent is kept if it matches or if any of its children match.
fn filter_declarations_by_kind(decls: &[Declaration], kind: &DeclKind) -> Vec<Declaration> {
    let mut result = Vec::new();
    for decl in decls {
        let filtered_children = filter_declarations_by_kind(&decl.children, kind);
        if decl.kind == *kind || !filtered_children.is_empty() {
            let mut filtered = decl.clone();
            filtered.children = filtered_children;
            result.push(filtered);
        }
    }
    result
}

/// Recursively filter declarations, removing those with Private visibility.
fn filter_declarations_by_visibility(decls: &[Declaration]) -> Vec<Declaration> {
    let mut result = Vec::new();
    for decl in decls {
        if matches!(decl.visibility, Visibility::Private) {
            continue;
        }
        let mut filtered = decl.clone();
        filtered.children = filter_declarations_by_visibility(&decl.children);
        result.push(filtered);
    }
    result
}

/// Recursively filter declarations by symbol name (case-insensitive substring).
/// A declaration is kept if its name matches or if any of its children match.
fn filter_declarations_by_symbol(decls: &[Declaration], query: &str) -> Vec<Declaration> {
    let mut result = Vec::new();
    for decl in decls {
        let name_matches = decl.name.to_lowercase().contains(query);
        let filtered_children = filter_declarations_by_symbol(&decl.children, query);
        if name_matches || !filtered_children.is_empty() {
            let mut filtered = decl.clone();
            if name_matches {
                // Keep all children if the parent matches
            } else {
                // Only keep matching children
                filtered.children = filtered_children;
            }
            result.push(filtered);
        }
    }
    result
}

/// Recalculate index stats after filtering.
fn recalculate_stats(index: &mut CodebaseIndex) {
    use std::collections::HashMap;

    let mut total_lines = 0;
    let mut languages: HashMap<String, usize> = HashMap::new();

    for file in &index.files {
        total_lines += file.lines;
        *languages
            .entry(file.language.name().to_string())
            .or_insert(0) += 1;
    }

    index.stats.total_files = index.files.len();
    index.stats.total_lines = total_lines;
    index.stats.languages = languages;
}
