pub mod queries;
pub mod tree_sitter_parser;

use std::path::Path;

use anyhow::Result;

use crate::languages::Language;
use crate::model::FileIndex;

pub trait LanguageParser: Send + Sync {
    fn language(&self) -> Language;
    fn parse_file(&self, path: &Path, content: &str) -> Result<FileIndex>;
}

pub struct ParserRegistry {
    parsers: Vec<Box<dyn LanguageParser>>,
}

impl ParserRegistry {
    pub fn new() -> Self {
        let mut registry = ParserRegistry {
            parsers: Vec::new(),
        };

        // Register Rust parser
        registry
            .parsers
            .push(Box::new(tree_sitter_parser::TreeSitterParser::new(
                Language::Rust,
            )));

        registry
    }

    pub fn get_parser(&self, language: &Language) -> Option<&dyn LanguageParser> {
        self.parsers
            .iter()
            .find(|p| &p.language() == language)
            .map(|p| p.as_ref())
    }
}
