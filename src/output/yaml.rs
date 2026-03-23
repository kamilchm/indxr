use anyhow::Result;

use crate::model::CodebaseIndex;
use crate::model::DetailLevel;

use super::OutputFormatter;

pub struct YamlFormatter;

impl OutputFormatter for YamlFormatter {
    fn format(&self, index: &CodebaseIndex, _detail: DetailLevel) -> Result<String> {
        Ok(serde_yaml::to_string(index)?)
    }
}
