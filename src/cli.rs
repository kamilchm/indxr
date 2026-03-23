use std::path::PathBuf;

use clap::Parser;

use crate::model::DetailLevel;

#[derive(Parser, Debug)]
#[command(name = "indxr", version, about = "Fast codebase indexer for AI agents")]
pub struct Cli {
    /// Root directory to index
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Output file path (default: stdout)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Output format: markdown, json, or yaml
    #[arg(short, long, default_value = "markdown")]
    pub format: OutputFormat,

    /// Detail level: summary, signatures, or full
    #[arg(short, long, default_value = "signatures")]
    pub detail: DetailLevel,

    /// Maximum directory depth to traverse
    #[arg(long)]
    pub max_depth: Option<usize>,

    /// Skip files larger than N kilobytes
    #[arg(long, default_value = "512")]
    pub max_file_size: u64,

    /// Comma-separated list of languages to include
    #[arg(short, long, value_delimiter = ',')]
    pub languages: Option<Vec<String>>,

    /// Additional glob patterns to exclude
    #[arg(short, long)]
    pub exclude: Option<Vec<String>>,

    /// Do not respect .gitignore
    #[arg(long)]
    pub no_gitignore: bool,

    /// Disable incremental caching
    #[arg(long)]
    pub no_cache: bool,

    /// Cache directory
    #[arg(long, default_value = ".indxr-cache")]
    pub cache_dir: PathBuf,

    /// Suppress progress output
    #[arg(short, long)]
    pub quiet: bool,

    /// Print indexing statistics to stderr
    #[arg(long)]
    pub stats: bool,
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum OutputFormat {
    Markdown,
    Json,
    Yaml,
}
