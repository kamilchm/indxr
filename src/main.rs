mod cache;
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
use rayon::prelude::*;

use crate::cache::Cache;
use crate::cli::{Cli, OutputFormat};
use crate::languages::Language;
use crate::model::{CodebaseIndex, IndexStats};
use crate::output::OutputFormatter;
use crate::output::markdown::MarkdownFormatter;
use crate::output::yaml::YamlFormatter;
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
    let exclude_patterns = cli.exclude.as_deref().unwrap_or(&[]);
    let walk_result = walker::walk_directory(
        &root,
        !cli.no_gitignore,
        cli.max_file_size,
        cli.max_depth,
        exclude_patterns,
    )?;

    // Filter by language if specified
    let lang_filter: Option<Vec<Language>> = cli.languages.as_ref().map(|langs| {
        langs
            .iter()
            .filter_map(|l| Language::from_name(l))
            .collect()
    });

    let files: Vec<&walker::FileEntry> = walk_result
        .files
        .iter()
        .filter(|f| {
            lang_filter
                .as_ref()
                .map_or(true, |filter| filter.contains(&f.language))
        })
        .collect();

    if !cli.quiet {
        eprintln!("Found {} files to index", files.len());
    }

    // Load cache
    let mut cache = if cli.no_cache {
        Cache::disabled()
    } else {
        Cache::load(&cli.cache_dir)
    };

    // Parse files in parallel
    let registry = ParserRegistry::new();

    struct ParseResult {
        file_index: model::FileIndex,
        relative_path: std::path::PathBuf,
        size: u64,
        mtime: u64,
        content_bytes: Option<Vec<u8>>,
    }

    let results: Vec<ParseResult> = files
        .par_iter()
        .filter_map(|file_entry| {
            // Check cache first
            if let Some(cached) = cache.get(
                &file_entry.relative_path,
                file_entry.size,
                file_entry.mtime,
            ) {
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
        .collect();

    // Update cache and collect results
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

    // Sort file indices by path for consistent output
    file_indices.sort_by(|a, b| a.path.cmp(&b.path));

    // Prune stale cache entries
    let existing: Vec<std::path::PathBuf> =
        files.iter().map(|f| f.relative_path.clone()).collect();
    cache.prune(&existing);
    cache.save()?;

    let duration = start.elapsed();

    if !cli.quiet && cache_hits > 0 {
        eprintln!(
            "{} files from cache, {} freshly parsed",
            cache_hits,
            file_indices.len() - cache_hits
        );
    }

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
            total_files: files.len(),
            total_lines,
            languages: language_counts,
            duration_ms: duration.as_millis() as u64,
        },
    };

    // Format output
    let formatted = match cli.format {
        OutputFormat::Markdown => {
            let formatter = MarkdownFormatter;
            formatter.format(&index, cli.detail)?
        }
        OutputFormat::Json => serde_json::to_string_pretty(&index)?,
        OutputFormat::Yaml => {
            let formatter = YamlFormatter;
            formatter.format(&index, cli.detail)?
        }
    };

    // Write output
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
        if !cli.no_cache {
            eprintln!("Cache: {} hits, {} entries", cache_hits, cache.len());
        }
        for (lang, count) in &index.stats.languages {
            eprintln!("  {}: {} files", lang, count);
        }
    }

    Ok(())
}
