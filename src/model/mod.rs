pub mod declarations;

use std::collections::HashMap;
use std::path::PathBuf;

use serde::Serialize;

use crate::languages::Language;
use self::declarations::Declaration;

#[derive(Debug, Serialize)]
pub struct CodebaseIndex {
    pub root: PathBuf,
    pub root_name: String,
    pub generated_at: String,
    pub files: Vec<FileIndex>,
    pub tree: Vec<TreeEntry>,
    pub stats: IndexStats,
}

#[derive(Debug, Serialize)]
pub struct FileIndex {
    pub path: PathBuf,
    pub language: Language,
    pub size: u64,
    pub lines: usize,
    pub imports: Vec<Import>,
    pub declarations: Vec<Declaration>,
}

#[derive(Debug, Clone, Serialize)]
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
