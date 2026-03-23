mod cli;
mod error;
mod languages;
mod model;
mod output;
mod parser;
mod walker;

use std::collections::HashMap;
use std::fs;
use std::time::Instant;

use anyhow::Result;
use clap::Parser;

use crate::cli::{Cli, OutputFormat};
use crate::model::{CodebaseIndex, IndexStats};
use crate::output::OutputFormatter;
use crate::output::markdown::MarkdownFormatter;
use crate::parser::ParserRegistry;

fn main() -> Result<()> {
    let cli = Cli::parse();
    let start = Instant::now();

    // Resolve the root path
    let root = fs::canonicalize(&cli.path)?;
    let root_name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "project".to_string());

    // Walk the directory
    let walk_result = walker::walk_directory(
        &root,
        !cli.no_gitignore,
        cli.max_file_size,
        cli.max_depth,
    )?;

    if !cli.quiet {
        eprintln!("Found {} files to index", walk_result.files.len());
    }

    // Parse files
    let registry = ParserRegistry::new();
    let mut file_indices = Vec::new();
    let mut total_lines = 0;
    let mut language_counts: HashMap<String, usize> = HashMap::new();

    for file_entry in &walk_result.files {
        if let Some(parser) = registry.get_parser(&file_entry.language) {
            match fs::read_to_string(&file_entry.path) {
                Ok(content) => {
                    match parser.parse_file(&file_entry.relative_path, &content) {
                        Ok(mut file_index) => {
                            file_index.size = file_entry.size;
                            total_lines += file_index.lines;
                            *language_counts
                                .entry(file_entry.language.name().to_string())
                                .or_insert(0) += 1;
                            file_indices.push(file_index);
                        }
                        Err(e) => {
                            if !cli.quiet {
                                eprintln!(
                                    "Warning: Failed to parse {}: {}",
                                    file_entry.relative_path.display(),
                                    e
                                );
                            }
                        }
                    }
                }
                Err(e) => {
                    if !cli.quiet {
                        eprintln!(
                            "Warning: Failed to read {}: {}",
                            file_entry.path.display(),
                            e
                        );
                    }
                }
            }
        }
    }

    let duration = start.elapsed();

    // Build the index
    let index = CodebaseIndex {
        root: root.clone(),
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
            duration_ms: duration.as_millis() as u64,
        },
    };

    // Format and write output
    let formatted = match cli.format {
        OutputFormat::Markdown => {
            let formatter = MarkdownFormatter;
            formatter.format(&index)?
        }
        OutputFormat::Json => serde_json::to_string_pretty(&index)?,
    };

    if let Some(output_path) = &cli.output {
        fs::write(output_path, &formatted)?;
        if !cli.quiet {
            eprintln!("Index written to {}", output_path.display());
        }
    } else {
        print!("{}", formatted);
    }

    // Print stats
    if cli.stats {
        eprintln!("\n--- Statistics ---");
        eprintln!("Files indexed: {}", index.stats.total_files);
        eprintln!("Total lines: {}", index.stats.total_lines);
        eprintln!("Duration: {}ms", index.stats.duration_ms);
        for (lang, count) in &index.stats.languages {
            eprintln!("  {}: {} files", lang, count);
        }
    }

    Ok(())
}
