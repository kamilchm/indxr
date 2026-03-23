use crate::model::CodebaseIndex;
use crate::model::declarations::{Declaration, Visibility};

/// Estimate the number of tokens in a string.
/// Uses the approximation: 1 token ~ 4 characters for English/code text.
pub fn estimate_tokens(text: &str) -> usize {
    (text.len() + 3) / 4
}

/// Estimate the total token count of a CodebaseIndex when rendered.
fn estimate_index_tokens(index: &CodebaseIndex) -> usize {
    let mut tokens = 0usize;

    // Header + stats
    tokens += 100;

    // Tree entries
    tokens += index.tree.len() * 5;

    // File sections
    for file in &index.files {
        // File section header
        tokens += 20;

        // Imports
        tokens += file.imports.len() * 8;

        // Declarations (recursive)
        tokens += estimate_declarations_tokens(&file.declarations);
    }

    tokens
}

/// Estimate tokens for a list of declarations recursively.
fn estimate_declarations_tokens(decls: &[Declaration]) -> usize {
    let mut tokens = 0usize;
    for decl in decls {
        // Base declaration cost (signature + kind)
        tokens += 15;

        // Doc comment cost
        if let Some(ref doc) = decl.doc_comment {
            tokens += estimate_tokens(doc);
        }

        // Children cost
        for child in &decl.children {
            tokens += 10;
            if let Some(ref doc) = child.doc_comment {
                tokens += estimate_tokens(doc);
            }
            // Recurse into children's children
            tokens += estimate_declarations_tokens(&child.children);
        }
    }
    tokens
}

/// Truncate a CodebaseIndex to fit within a token budget.
/// Strategy:
/// 1. Always include the header and directory tree
/// 2. Include public API surface first (public declarations)
/// 3. Include private declarations if budget allows
/// 4. Truncate doc comments if needed
/// 5. Drop children (fields, methods) last
pub fn apply_token_budget(index: &mut CodebaseIndex, max_tokens: usize) {
    let current = estimate_index_tokens(index);
    if current <= max_tokens {
        return;
    }

    // Step 1: Remove all doc comments
    for file in &mut index.files {
        strip_doc_comments(&mut file.declarations);
    }

    let current = estimate_index_tokens(index);
    if current <= max_tokens {
        return;
    }

    // Step 2: Remove all private declarations
    for file in &mut index.files {
        file.declarations = remove_private_declarations(&file.declarations);
    }
    // Remove files that now have no declarations (and no imports)
    index.files.retain(|f| !f.declarations.is_empty() || !f.imports.is_empty());

    let current = estimate_index_tokens(index);
    if current <= max_tokens {
        return;
    }

    // Step 3: Remove all children from declarations
    for file in &mut index.files {
        strip_children(&mut file.declarations);
    }

    let current = estimate_index_tokens(index);
    if current <= max_tokens {
        return;
    }

    // Step 4: Drop files from the end (files are sorted by path) until we fit
    // Always keep at least one file
    while index.files.len() > 1 {
        let current = estimate_index_tokens(index);
        if current <= max_tokens {
            break;
        }
        index.files.pop();
    }
}

/// Recursively strip all doc comments from declarations and their children.
fn strip_doc_comments(decls: &mut [Declaration]) {
    for decl in decls.iter_mut() {
        decl.doc_comment = None;
        strip_doc_comments(&mut decl.children);
    }
}

/// Recursively remove private declarations, keeping public and pub(crate) ones.
fn remove_private_declarations(decls: &[Declaration]) -> Vec<Declaration> {
    let mut result = Vec::new();
    for decl in decls {
        if matches!(decl.visibility, Visibility::Private) {
            continue;
        }
        let mut filtered = decl.clone();
        filtered.children = remove_private_declarations(&decl.children);
        result.push(filtered);
    }
    result
}

/// Remove all children from declarations recursively.
fn strip_children(decls: &mut [Declaration]) {
    for decl in decls.iter_mut() {
        decl.children.clear();
    }
}
