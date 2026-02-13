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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_messages() {
        let cases = vec![
            (Error::Config("bad".into()), "configuration error: bad"),
            (Error::Indexer("rpc".into()), "indexer error: rpc"),
            (Error::Feature("feat".into()), "feature extraction error: feat"),
            (Error::Scoring("score".into()), "scoring error: score"),
            (Error::Alert("alert".into()), "alert error: alert"),
            (Error::Api("api".into()), "api error: api"),
            (Error::Storage("db".into()), "storage error: db"),
            (Error::ChannelClosed, "channel closed unexpectedly"),
        ];
        for (err, expected) in cases {
            assert_eq!(format!("{err}"), expected);
        }
    }

    #[test]
    fn io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "gone");
        let err: Error = io_err.into();
        assert!(matches!(err, Error::Io(_)));
        assert!(format!("{err}").contains("gone"));
    }

    #[test]
    fn serde_error_conversion() {
        let result: Result<serde_json::Value, _> = serde_json::from_str("not json");
        let serde_err = result.unwrap_err();
        let err: Error = serde_err.into();
        assert!(matches!(err, Error::SerdeJson(_)));
    }
}
