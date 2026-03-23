use std::fmt::Write;

use anyhow::Result;

use crate::model::CodebaseIndex;
use crate::model::DetailLevel;
use crate::model::declarations::{DeclKind, Declaration, Visibility};

use super::OutputFormatter;

pub struct MarkdownFormatter;

impl OutputFormatter for MarkdownFormatter {
    fn format(&self, index: &CodebaseIndex, detail: DetailLevel) -> Result<String> {
        let mut out = String::new();

        // Header
        writeln!(out, "# Codebase Index: {}", index.root_name)?;
        writeln!(out)?;

        // Stats
        let mut lang_summary: Vec<String> = index
            .stats
            .languages
            .iter()
            .map(|(lang, count)| format!("{} ({})", lang, count))
            .collect();
        lang_summary.sort();
        writeln!(
            out,
            "> Generated: {} | Files: {} | Lines: {}",
            index.generated_at, index.stats.total_files, index.stats.total_lines
        )?;
        writeln!(out, "> Languages: {}", lang_summary.join(", "))?;
        writeln!(out)?;

        // Directory tree
        writeln!(out, "## Directory Structure")?;
        writeln!(out)?;
        writeln!(out, "```")?;
        writeln!(out, "{}/", index.root_name)?;
        for entry in &index.tree {
            let indent = "  ".repeat(entry.depth);
            if entry.is_dir {
                writeln!(out, "{}{}/", indent, entry.path)?;
            } else {
                writeln!(out, "{}{}", indent, entry.path)?;
            }
        }
        writeln!(out, "```")?;
        writeln!(out)?;

        // Summary mode stops here
        if detail == DetailLevel::Summary {
            return Ok(out);
        }

        // Public API surface section
        let has_public = index.files.iter().any(|f| {
            f.declarations
                .iter()
                .any(|d| matches!(d.visibility, Visibility::Public))
        });

        if has_public {
            writeln!(out, "---")?;
            writeln!(out)?;
            writeln!(out, "## Public API Surface")?;
            writeln!(out)?;
            for file in &index.files {
                let public_decls: Vec<&Declaration> = file
                    .declarations
                    .iter()
                    .filter(|d| matches!(d.visibility, Visibility::Public) && !matches!(d.kind, DeclKind::Impl))
                    .collect();
                if public_decls.is_empty() {
                    continue;
                }
                writeln!(out, "**{}**", file.path.display())?;
                for decl in &public_decls {
                    write!(out, "- `{}`", decl.signature)?;
                    write_badges(&mut out, decl)?;
                    writeln!(out)?;
                }
                writeln!(out)?;
            }
        }

        // File sections
        for file in &index.files {
            writeln!(out, "---")?;
            writeln!(out)?;
            writeln!(out, "## {}", file.path.display())?;
            writeln!(out)?;
            writeln!(
                out,
                "**Language:** {} | **Size:** {} | **Lines:** {}",
                file.language,
                format_size(file.size),
                file.lines
            )?;
            writeln!(out)?;

            // Imports
            if !file.imports.is_empty() {
                writeln!(out, "**Imports:**")?;
                for import in &file.imports {
                    writeln!(out, "- `{}`", import.text)?;
                }
                writeln!(out)?;
            }

            // Declarations
            if !file.declarations.is_empty() {
                writeln!(out, "**Declarations:**")?;
                writeln!(out)?;
                for decl in &file.declarations {
                    format_declaration(&mut out, decl, 0, detail)?;
                }
            }
        }

        Ok(out)
    }
}

/// Write metadata badges for a declaration (test, async, deprecated)
fn write_badges(out: &mut String, decl: &Declaration) -> std::fmt::Result {
    let mut badges = Vec::new();
    if decl.is_test {
        badges.push("test");
    }
    if decl.is_async {
        badges.push("async");
    }
    if decl.is_deprecated {
        badges.push("deprecated");
    }
    if !badges.is_empty() {
        write!(out, " [{}]", badges.join(", "))?;
    }
    Ok(())
}

fn format_declaration(
    out: &mut String,
    decl: &Declaration,
    depth: usize,
    detail: DetailLevel,
) -> std::fmt::Result {
    let indent = "  ".repeat(depth);

    match decl.kind {
        DeclKind::Impl => {
            writeln!(out, "{}**`{}`**", indent, decl.signature)?;
        }
        DeclKind::Field | DeclKind::Variant => {
            writeln!(out, "{}- `{}`", indent, decl.signature)?;
            return Ok(());
        }
        _ => {
            let vis = match &decl.visibility {
                Visibility::Public => "pub ",
                Visibility::PublicCrate => "pub(crate) ",
                Visibility::Private => "",
            };

            // Avoid duplicating visibility in signature
            let sig = if decl.signature.starts_with("pub ")
                || decl.signature.starts_with("pub(")
                || decl.signature.starts_with("export ")
            {
                decl.signature.clone()
            } else {
                format!("{}{}", vis, decl.signature)
            };

            write!(out, "{}`{}`", indent, sig)?;
            write_badges(out, decl)?;
            writeln!(out)?;
        }
    }

    // Doc comment (shown in signatures and full modes)
    if let Some(doc) = &decl.doc_comment {
        writeln!(out, "{}> {}", indent, doc)?;
    }

    // Line number (shown in signatures and full modes)
    if detail == DetailLevel::Full || detail == DetailLevel::Signatures {
        if decl.kind != DeclKind::Impl && decl.line > 0 {
            write!(out, "{}> Line {}", indent, decl.line)?;
            if let Some(body) = decl.body_lines {
                write!(out, " ({} lines)", body)?;
            }
            writeln!(out)?;
        }
    }

    // Relationships
    if !decl.relationships.is_empty() {
        let rels: Vec<String> = decl
            .relationships
            .iter()
            .map(|r| format!("{} `{}`", r.kind, r.target))
            .collect();
        writeln!(out, "{}> {}", indent, rels.join(", "))?;
    }

    // Children
    if !decl.children.is_empty() {
        match decl.kind {
            DeclKind::Struct | DeclKind::Class => {
                let fields: Vec<String> = decl
                    .children
                    .iter()
                    .filter(|c| c.kind == DeclKind::Field)
                    .map(|f| format!("`{}`", f.signature))
                    .collect();
                if !fields.is_empty() {
                    writeln!(out, "{}> Fields: {}", indent, fields.join(", "))?;
                }
                // Methods inside class/struct
                for child in &decl.children {
                    if child.kind == DeclKind::Method || child.kind == DeclKind::Function {
                        format_declaration(out, child, depth + 1, detail)?;
                    }
                }
            }
            DeclKind::Enum => {
                let variants: Vec<String> = decl
                    .children
                    .iter()
                    .filter(|c| c.kind == DeclKind::Variant)
                    .map(|v| format!("`{}`", v.name))
                    .collect();
                if !variants.is_empty() {
                    writeln!(out, "{}> Variants: {}", indent, variants.join(", "))?;
                }
            }
            DeclKind::Impl | DeclKind::Trait | DeclKind::Module => {
                for child in &decl.children {
                    format_declaration(out, child, depth + 1, detail)?;
                }
            }
            DeclKind::Message | DeclKind::Service | DeclKind::SchemaType | DeclKind::Interface => {
                for child in &decl.children {
                    format_declaration(out, child, depth + 1, detail)?;
                }
            }
            DeclKind::Heading => {
                // Headings with children (sub-headings)
                for child in &decl.children {
                    format_declaration(out, child, depth + 1, detail)?;
                }
            }
            DeclKind::ConfigKey => {
                // Config keys with children (nested keys)
                for child in &decl.children {
                    format_declaration(out, child, depth + 1, detail)?;
                }
            }
            DeclKind::TableDef => {
                // Table columns
                let cols: Vec<String> = decl
                    .children
                    .iter()
                    .filter(|c| c.kind == DeclKind::Field)
                    .map(|f| format!("`{}`", f.signature))
                    .collect();
                if !cols.is_empty() {
                    writeln!(out, "{}> Columns: {}", indent, cols.join(", "))?;
                }
            }
            _ => {}
        }
    }

    writeln!(out)?;
    Ok(())
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

impl std::fmt::Display for crate::model::declarations::RelKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            crate::model::declarations::RelKind::Implements => write!(f, "implements"),
            crate::model::declarations::RelKind::Extends => write!(f, "extends"),
        }
    }
}
