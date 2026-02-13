use serde::{Deserialize, Serialize};
use std::path::Path;

/// Top-level application configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub pipeline: PipelineConfig,
    pub indexer: IndexerConfig,
    pub scorer: ScorerConfig,
    pub alerts: AlertsConfig,
    pub api: ApiConfig,
    pub storage: StorageConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    /// Channel buffer size between pipeline stages.
    pub channel_buffer: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexerConfig {
    /// Ethereum JSON-RPC endpoint (HTTP or WebSocket).
    pub rpc_url: String,
    /// Polling interval in milliseconds (used for HTTP provider).
    pub poll_interval_ms: u64,
    /// Optional starting block number; `None` means latest.
    pub start_block: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScorerConfig {
    /// Anomaly score threshold that triggers an alert (0.0 – 1.0).
    pub alert_threshold: f64,
    /// Weights for ensemble models (isolation-forest, statistical, rules).
    pub model_weights: ModelWeights,
    /// Isolation forest configuration.
    pub isolation_forest: IsolationForestConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsolationForestConfig {
    /// Number of isolation trees in the forest.
    pub num_trees: usize,
    /// Number of samples used to build each tree.
    pub sample_size: usize,
    /// Number of feature vectors to collect before the first training.
    pub min_training_samples: usize,
    /// Re-train the model every N scored transactions.
    pub retrain_interval: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Path to the SQLite database file.
    pub db_path: String,
    /// Whether storage is enabled.
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelWeights {
    pub isolation_forest: f64,
    pub statistical: f64,
    pub rules: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertsConfig {
    /// Enable logging alerts to stdout.
    pub log_enabled: bool,
    /// Optional webhook URL to POST alerts to.
    pub webhook_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    /// Address to bind the HTTP server to.
    pub bind_address: String,
    /// Port for the HTTP server.
    pub port: u16,
}

impl AppConfig {
    /// Load configuration from a TOML file.
    pub fn from_file(path: &Path) -> Result<Self, crate::error::Error> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| crate::error::Error::Config(format!("cannot read config: {e}")))?;
        let config: Self = toml::from_str(&content)
            .map_err(|e| crate::error::Error::Config(format!("invalid config TOML: {e}")))?;
        Ok(config)
    }

    /// Load from the default path `config/default.toml`.
    pub fn load_default() -> Result<Self, crate::error::Error> {
        Self::from_file(Path::new("config/default.toml"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn parse_valid_toml() {
        let dir = std::env::temp_dir().join("fraud_test_config_valid");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.toml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, r#"
[pipeline]
channel_buffer = 64

[indexer]
rpc_url = "http://localhost:8545"
poll_interval_ms = 1000

[scorer]
alert_threshold = 0.7

[scorer.model_weights]
isolation_forest = 0.4
statistical = 0.35
rules = 0.25

[scorer.isolation_forest]
num_trees = 10
sample_size = 32
min_training_samples = 64
retrain_interval = 100

[alerts]
log_enabled = true

[api]
bind_address = "127.0.0.1"
port = 3000

[storage]
enabled = false
db_path = ":memory:"
"#).unwrap();

        let config = AppConfig::from_file(&path).unwrap();
        assert_eq!(config.pipeline.channel_buffer, 64);
        assert_eq!(config.scorer.alert_threshold, 0.7);
        assert!(!config.storage.enabled);
        assert_eq!(config.scorer.isolation_forest.num_trees, 10);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_file_returns_error() {
        let result = AppConfig::from_file(Path::new("/tmp/nonexistent_fraud_cfg.toml"));
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("cannot read config"));
    }

    #[test]
    fn invalid_toml_returns_error() {
        let dir = std::env::temp_dir().join("fraud_test_config_invalid");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bad.toml");
        std::fs::write(&path, "this is not valid toml {{{{").unwrap();

        let result = AppConfig::from_file(&path);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("invalid config TOML"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn incomplete_toml_returns_error() {
        let dir = std::env::temp_dir().join("fraud_test_config_incomplete");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("partial.toml");
        std::fs::write(&path, "[pipeline]\nchannel_buffer = 64\n").unwrap();

        let result = AppConfig::from_file(&path);
        assert!(result.is_err());

        std::fs::remove_dir_all(&dir).ok();
    }
}
