pub mod declarations;

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::languages::Language;
use self::declarations::Declaration;

#[derive(Debug, Clone, Copy, PartialEq, clap::ValueEnum)]
pub enum DetailLevel {
    Summary,
    Signatures,
    Full,
}

#[derive(Debug, Serialize)]
pub struct CodebaseIndex {
    pub root: PathBuf,
    pub root_name: String,
    pub generated_at: String,
    pub files: Vec<FileIndex>,
    pub tree: Vec<TreeEntry>,
    pub stats: IndexStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileIndex {
    pub path: PathBuf,
    pub language: Language,
    pub size: u64,
    pub lines: usize,
    pub imports: Vec<Import>,
    pub declarations: Vec<Declaration>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Import {
    pub text: String,
}

#[derive(Debug, Serialize)]
pub struct TreeEntry {
    pub path: String,
    pub is_dir: bool,
    pub depth: usize,
}

#[derive(Debug, Serialize)]
pub struct IndexStats {
    pub total_files: usize,
    pub total_lines: usize,
    pub languages: HashMap<String, usize>,
    pub duration_ms: u64,
}
