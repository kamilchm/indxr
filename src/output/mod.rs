pub mod markdown;

use anyhow::Result;

use crate::model::CodebaseIndex;

pub trait OutputFormatter {
    fn format(&self, index: &CodebaseIndex) -> Result<String>;
}
