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
        use tree_sitter_parser::TreeSitterParser;

        let languages = [
            Language::Rust,
            Language::Python,
            Language::TypeScript,
            Language::JavaScript,
            Language::Go,
            Language::Java,
            Language::C,
            Language::Cpp,
        ];

        let parsers: Vec<Box<dyn LanguageParser>> = languages
            .into_iter()
            .map(|lang| Box::new(TreeSitterParser::new(lang)) as Box<dyn LanguageParser>)
            .collect();

        ParserRegistry { parsers }
    }

    pub fn get_parser(&self, language: &Language) -> Option<&dyn LanguageParser> {
        self.parsers
            .iter()
            .find(|p| &p.language() == language)
            .map(|p| p.as_ref())
    }
}
