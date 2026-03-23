use std::path::PathBuf;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "indxr", version, about = "Fast codebase indexer for AI agents")]
pub struct Cli {
    /// Root directory to index
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Output file path (default: stdout)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Output format: markdown or json
    #[arg(short, long, default_value = "markdown")]
    pub format: OutputFormat,

    /// Maximum directory depth to traverse
    #[arg(long)]
    pub max_depth: Option<usize>,

    /// Skip files larger than N kilobytes
    #[arg(long, default_value = "512")]
    pub max_file_size: u64,

    /// Do not respect .gitignore
    #[arg(long)]
    pub no_gitignore: bool,

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
}
