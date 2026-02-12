use thiserror::Error;

/// Unified error type used across the fraud-detection pipeline.
#[derive(Debug, Error)]
pub enum Error {
    #[error("configuration error: {0}")]
    Config(String),

    #[error("indexer error: {0}")]
    Indexer(String),

    #[error("feature extraction error: {0}")]
    Feature(String),

    #[error("scoring error: {0}")]
    Scoring(String),

    #[error("alert error: {0}")]
    Alert(String),

    #[error("api error: {0}")]
    Api(String),

    #[error("storage error: {0}")]
    Storage(String),

    #[error("channel closed unexpectedly")]
    ChannelClosed,

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
}
