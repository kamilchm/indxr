use thiserror::Error;

#[derive(Error, Debug)]
pub enum IndxrError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Parse error in {path}: {message}")]
    Parse { path: String, message: String },

    #[error("Unsupported language: {0}")]
    UnsupportedLanguage(String),
}
