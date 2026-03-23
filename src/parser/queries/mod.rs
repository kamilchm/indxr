pub mod rust;

use crate::languages::Language;
use crate::model::Import;
use crate::model::declarations::Declaration;

pub trait DeclExtractor: Send + Sync {
    fn extract(&self, root: tree_sitter::Node<'_>, source: &str) -> (Vec<Import>, Vec<Declaration>);
}

pub fn get_extractor(language: &Language) -> Box<dyn DeclExtractor> {
    match language {
        Language::Rust => Box::new(rust::RustExtractor),
        _ => panic!("No extractor for language: {}", language),
    }
}
