//! Error types for kanji conversion

use std::path::PathBuf;

/// Errors that can occur during kanji conversion operations.
#[derive(Debug, thiserror::Error)]
pub enum KanjiError {
    #[error("model file not found: {0}")]
    ModelNotFound(PathBuf),

    #[error("tokenizer.json not found: place tokenizer.json next to the GGUF (expected {0})")]
    TokenizerNotFound(PathBuf),

    #[error("download failed")]
    Download(#[source] Box<dyn std::error::Error + Send + Sync>),

    #[error("model load failed")]
    ModelLoad(#[source] Box<dyn std::error::Error + Send + Sync>),

    #[error("tokenizer load failed")]
    TokenizerLoad(#[source] Box<dyn std::error::Error + Send + Sync>),

    #[error("inference failed")]
    Inference(#[source] Box<dyn std::error::Error + Send + Sync>),
}

pub type Result<T> = std::result::Result<T, KanjiError>;
