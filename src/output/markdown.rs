use std::fmt::Write;

use anyhow::Result;

use crate::model::CodebaseIndex;
use crate::model::declarations::{DeclKind, Declaration, Visibility};

use super::OutputFormatter;

pub struct MarkdownFormatter;

impl OutputFormatter for MarkdownFormatter {
    fn format(&self, index: &CodebaseIndex) -> Result<String> {
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
                    format_declaration(&mut out, decl, 0)?;
                }
            }
        }

        Ok(out)
    }
}

fn format_declaration(out: &mut String, decl: &Declaration, depth: usize) -> std::fmt::Result {
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
            let sig = if decl.signature.starts_with("pub ") || decl.signature.starts_with("pub(") {
                decl.signature.clone()
            } else {
                format!("{}{}", vis, decl.signature)
            };

            writeln!(out, "{}`{}`", indent, sig)?;
        }
    }

    // Doc comment
    if let Some(doc) = &decl.doc_comment {
        writeln!(out, "{}> {}", indent, doc)?;
    }

    // Children
    if !decl.children.is_empty() {
        match decl.kind {
            DeclKind::Struct => {
                let fields: Vec<String> = decl
                    .children
                    .iter()
                    .map(|f| format!("`{}`", f.signature))
                    .collect();
                writeln!(out, "{}> Fields: {}", indent, fields.join(", "))?;
            }
            DeclKind::Enum => {
                let variants: Vec<String> =
                    decl.children.iter().map(|v| format!("`{}`", v.name)).collect();
                writeln!(out, "{}> Variants: {}", indent, variants.join(", "))?;
            }
            DeclKind::Impl | DeclKind::Trait => {
                for child in &decl.children {
                    format_declaration(out, child, depth + 1)?;
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
