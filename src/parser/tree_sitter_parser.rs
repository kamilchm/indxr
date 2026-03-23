use std::path::Path;

use anyhow::{Context, Result};

use crate::languages::Language;
use crate::model::FileIndex;

use super::LanguageParser;
use super::queries;

pub struct TreeSitterParser {
    language: Language,
}

impl TreeSitterParser {
    pub fn new(language: Language) -> Self {
        Self { language }
    }

    fn get_ts_language(&self) -> tree_sitter::Language {
        match self.language {
            Language::Rust => tree_sitter_rust::LANGUAGE.into(),
            _ => panic!("Unsupported language: {:?}", self.language),
        }
    }
}

impl LanguageParser for TreeSitterParser {
    fn language(&self) -> Language {
        self.language.clone()
    }

    fn parse_file(&self, path: &Path, content: &str) -> Result<FileIndex> {
        let mut parser = tree_sitter::Parser::new();
        let ts_lang = self.get_ts_language();
        parser
            .set_language(&ts_lang)
            .map_err(|e| anyhow::anyhow!("Failed to set tree-sitter language: {:?}", e))?;

        let tree = parser
            .parse(content, None)
            .context("Failed to parse file")?;

        let root = tree.root_node();
        let extractor = queries::get_extractor(&self.language);
        let (imports, declarations) = extractor.extract(root, content);

        let lines = content.lines().count();

        Ok(FileIndex {
            path: path.to_path_buf(),
            language: self.language.clone(),
            size: content.len() as u64,
            lines,
            imports,
            declarations,
        })
    }
}
