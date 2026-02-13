use fraud_common::config::ScorerConfig;
use fraud_common::types::RiskLevel;
use fraud_features::FeatureExtractor;
use fraud_scorer::EnsembleScorer;
use tracing::info;

use crate::dataset::Dataset;

// ---------------------------------------------------------------------------
// Prediction result
// ---------------------------------------------------------------------------

/// Result of scoring a single labeled transaction.
#[derive(Debug, Clone)]
pub struct PredictionResult {
    pub tx_hash: String,
    /// Ensemble fraud score in \[0, 1\].
    pub score: f64,
    /// Assigned risk level.
    pub risk_level: RiskLevel,
    /// Ground-truth label from the dataset.
    pub is_fraud: bool,
    /// Names of rules that fired.
    pub triggered_rules: Vec<String>,
}

// ---------------------------------------------------------------------------
// Replay engine
// ---------------------------------------------------------------------------

/// Engine that replays a labeled dataset through the feature extractor and
/// ensemble scorer, collecting predictions for evaluation.
pub struct ReplayEngine {
    extractor: FeatureExtractor,
    scorer: EnsembleScorer,
}

impl ReplayEngine {
    pub fn new(scorer_config: &ScorerConfig) -> Self {
        Self {
            extractor: FeatureExtractor::new(),
            scorer: EnsembleScorer::new(scorer_config),
        }
    }

    /// Replay all transactions in the dataset through the pipeline.
    ///
    /// Transactions are processed sequentially so that rolling statistics and
    /// the address graph accumulate naturally, just like in production.
    pub fn replay(&mut self, dataset: &Dataset) -> Vec<PredictionResult> {
        let mut results = Vec::with_capacity(dataset.len());

        for (i, entry) in dataset.entries.iter().enumerate() {
            let fv = self.extractor.extract(&entry.tx);
            let fraud_score = self.scorer.score(&fv);

            results.push(PredictionResult {
                tx_hash: entry.tx.hash.clone(),
                score: fraud_score.score,
                risk_level: fraud_score.risk_level,
                is_fraud: entry.is_fraud,
                triggered_rules: fraud_score.triggered_rules,
            });

            if (i + 1) % 500 == 0 || i + 1 == dataset.len() {
                info!(
                    progress = i + 1,
                    total = dataset.len(),
                    "replay progress"
                );
            }
        }

        info!(total = results.len(), "replay complete");
        results
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dataset::LabeledTransaction;
    use fraud_common::config::{IsolationForestConfig, ModelWeights, ScorerConfig};
    use fraud_common::types::Transaction;

    fn test_config() -> ScorerConfig {
        ScorerConfig {
            alert_threshold: 0.65,
            model_weights: ModelWeights {
                isolation_forest: 0.4,
                statistical: 0.35,
                rules: 0.25,
            },
            isolation_forest: IsolationForestConfig {
                num_trees: 10,
                sample_size: 32,
                min_training_samples: 1000,
                retrain_interval: 5000,
            },
        }
    }

    fn make_tx(hash: &str, value_eth: f64, nonce: u64, to: Option<&str>) -> Transaction {
        Transaction {
            hash: hash.into(),
            block_number: 100,
            from: "0xsender".into(),
            to: to.map(|s| s.into()),
            value: (value_eth * 1e18) as u128,
            gas_price: Some(20_000_000_000),
            max_fee_per_gas: Some(20_000_000_000),
            max_priority_fee_per_gas: None,
            gas_used: 21_000,
            input: vec![],
            nonce,
            tx_index: 0,
            timestamp: 1_700_000_000,
        }
    }

    fn small_dataset() -> Dataset {
        let mut entries = Vec::new();
        // 5 legit
        for i in 0..5 {
            entries.push(LabeledTransaction {
                tx: make_tx(&format!("0x{i}"), 0.5 + i as f64 * 0.1, 5 + i, Some("0xto")),
                is_fraud: false,
            });
        }
        // 2 fraud – high value, nonce 0
        entries.push(LabeledTransaction {
            tx: make_tx("0xfraud1", 500.0, 0, Some("0xto")),
            is_fraud: true,
        });
        entries.push(LabeledTransaction {
            tx: make_tx("0xfraud2", 200.0, 0, Some("0xto")),
            is_fraud: true,
        });
        Dataset::from_entries(entries)
    }

    #[test]
    fn replay_produces_correct_count() {
        let ds = small_dataset();
        let mut engine = ReplayEngine::new(&test_config());
        let results = engine.replay(&ds);
        assert_eq!(results.len(), ds.len());
    }

    #[test]
    fn scores_in_valid_range() {
        let ds = small_dataset();
        let mut engine = ReplayEngine::new(&test_config());
        let results = engine.replay(&ds);
        for r in &results {
            assert!(
                r.score >= 0.0 && r.score <= 1.0,
                "score out of range: {} for {}",
                r.score,
                r.tx_hash
            );
        }
    }

    #[test]
    fn fraud_preserves_labels() {
        let ds = small_dataset();
        let mut engine = ReplayEngine::new(&test_config());
        let results = engine.replay(&ds);

        let fraud_count = results.iter().filter(|r| r.is_fraud).count();
        assert_eq!(fraud_count, 2);
        assert_eq!(results.len() - fraud_count, 5);
    }

    #[test]
    fn high_value_fraud_triggers_rules() {
        let ds = small_dataset();
        let mut engine = ReplayEngine::new(&test_config());
        let results = engine.replay(&ds);

        // The 500 ETH fraud tx should trigger high_value_transfer
        let fraud500 = results.iter().find(|r| r.tx_hash == "0xfraud1").unwrap();
        assert!(
            fraud500
                .triggered_rules
                .contains(&"high_value_transfer".to_string()),
            "expected high_value_transfer rule for 500 ETH tx, got: {:?}",
            fraud500.triggered_rules
        );
    }
}
