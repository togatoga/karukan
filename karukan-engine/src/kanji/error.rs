//! Error types for kanji conversion

/// Errors that can occur during kanji conversion operations.
#[derive(Debug, thiserror::Error)]
pub enum KanjiError {
    #[error("unknown model variant: '{0}'")]
    UnknownVariant(String),

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
