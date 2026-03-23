pub mod markdown;
pub mod yaml;

use anyhow::Result;

use crate::model::CodebaseIndex;
use crate::model::DetailLevel;

pub trait OutputFormatter {
    fn format(&self, index: &CodebaseIndex, detail: DetailLevel) -> Result<String>;
}
