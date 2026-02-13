use chrono::Utc;
use fraud_common::config::ScorerConfig;
use fraud_common::error::Error;
use fraud_common::types::{FraudScore, PipelineEvent, RiskLevel};
use tokio::sync::mpsc;
use tracing::{info, warn};
use uuid::Uuid;

use crate::model::{IsolationForestModel, StatisticalModel};
use crate::rules::RuleEngine;

/// Ensemble scorer that combines ML models + heuristic rules into a single
/// fraud score per transaction.
pub struct EnsembleScorer {
    isolation_forest: IsolationForestModel,
    statistical: StatisticalModel,
    rules: RuleEngine,
    weights: Weights,
    alert_threshold: f64,
}

struct Weights {
    isolation_forest: f64,
    statistical: f64,
    rules: f64,
}

impl EnsembleScorer {
    pub fn new(config: &ScorerConfig) -> Self {
        Self {
            isolation_forest: IsolationForestModel::new(&config.isolation_forest),
            statistical: StatisticalModel::new(),
            rules: RuleEngine::new(),
            weights: Weights {
                isolation_forest: config.model_weights.isolation_forest,
                statistical: config.model_weights.statistical,
                rules: config.model_weights.rules,
            },
            alert_threshold: config.alert_threshold,
        }
    }

    /// Score a single feature vector. Returns `Some(FraudScore)` for every
    /// transaction, letting the caller decide what to do with low-risk ones.
    pub fn score(
        &mut self,
        fv: &fraud_common::types::FeatureVector,
    ) -> FraudScore {
        let mut if_score = self.isolation_forest.score(fv);
        if_score.weight = self.weights.isolation_forest;

        let mut stat_score = self.statistical.score(fv);
        stat_score.weight = self.weights.statistical;

        let (mut rule_score, triggered_rules) = self.rules.evaluate(fv);
        rule_score.weight = self.weights.rules;

        let total_weight =
            if_score.weight + stat_score.weight + rule_score.weight;
        let weighted_score = if total_weight > 0.0 {
            (if_score.score * if_score.weight
                + stat_score.score * stat_score.weight
                + rule_score.score * rule_score.weight)
                / total_weight
        } else {
            0.0
        };

        let risk_level = risk_from_score(weighted_score);

        FraudScore {
            id: Uuid::new_v4(),
            tx_hash: fv.tx_hash.clone(),
            block_number: fv.block_number,
            score: weighted_score,
            risk_level,
            model_scores: vec![if_score, stat_score, rule_score],
            triggered_rules,
            scored_at: Utc::now(),
        }
    }

    /// Pipeline stage: consumes `FeaturesExtracted` events from `rx`, scores
    /// each, and forwards `Scored` events (and `AlertRaised` for high scores)
    /// to `tx`.
    pub async fn run(
        mut self,
        mut rx: mpsc::Receiver<PipelineEvent>,
        tx: mpsc::Sender<PipelineEvent>,
    ) -> Result<(), Error> {
        info!(threshold = self.alert_threshold, "ensemble scorer started");

        while let Some(event) = rx.recv().await {
            if let PipelineEvent::FeaturesExtracted(fv) = event {
                let fraud_score = self.score(&fv);

                // Always emit the scored event
                let scored = PipelineEvent::Scored(fraud_score.clone());
                if tx.send(scored).await.is_err() {
                    warn!("downstream channel closed");
                    return Err(Error::ChannelClosed);
                }

                // Emit alert if above threshold
                if fraud_score.score >= self.alert_threshold {
                    let alert = fraud_common::types::Alert {
                        id: Uuid::new_v4(),
                        fraud_score,
                        from: String::new(), // filled by caller if needed
                        to: None,
                        value: 0,
                        created_at: Utc::now(),
                        acknowledged: false,
                    };
                    let alert_event = PipelineEvent::AlertRaised(alert);
                    if tx.send(alert_event).await.is_err() {
                        warn!("downstream channel closed");
                        return Err(Error::ChannelClosed);
                    }
                }
            }
        }

        Ok(())
    }
}

fn risk_from_score(score: f64) -> RiskLevel {
    match score {
        s if s >= 0.85 => RiskLevel::Critical,
        s if s >= 0.65 => RiskLevel::High,
        s if s >= 0.40 => RiskLevel::Medium,
        _ => RiskLevel::Low,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use fraud_common::config::{IsolationForestConfig, ModelWeights, ScorerConfig};
    use fraud_common::types::{Feature, FeatureVector};

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
                min_training_samples: 1000, // keep in warm-up for tests
                retrain_interval: 5000,
            },
        }
    }

    fn make_fv(features: Vec<(&str, f64)>) -> FeatureVector {
        FeatureVector {
            tx_hash: "0xtest".into(),
            block_number: 1,
            features: features.into_iter().map(|(n, v)| Feature { name: n.into(), value: v }).collect(),
            extracted_at: Utc::now(),
        }
    }

    #[test]
    fn risk_from_score_thresholds() {
        assert_eq!(risk_from_score(0.0), RiskLevel::Low);
        assert_eq!(risk_from_score(0.39), RiskLevel::Low);
        assert_eq!(risk_from_score(0.40), RiskLevel::Medium);
        assert_eq!(risk_from_score(0.64), RiskLevel::Medium);
        assert_eq!(risk_from_score(0.65), RiskLevel::High);
        assert_eq!(risk_from_score(0.84), RiskLevel::High);
        assert_eq!(risk_from_score(0.85), RiskLevel::Critical);
        assert_eq!(risk_from_score(1.0), RiskLevel::Critical);
    }

    #[test]
    fn ensemble_produces_valid_fraud_score() {
        let mut scorer = EnsembleScorer::new(&test_config());
        let fv = make_fv(vec![
            ("value_eth", 1.0),
            ("gas_used", 21000.0),
            ("value_z_score", 0.5),
            ("gas_z_score", 0.3),
        ]);
        let result = scorer.score(&fv);
        assert_eq!(result.tx_hash, "0xtest");
        assert!(result.score >= 0.0 && result.score <= 1.0);
        assert_eq!(result.model_scores.len(), 3);
        assert_eq!(result.model_scores[0].model_name, "isolation_forest");
        assert_eq!(result.model_scores[1].model_name, "statistical");
        assert_eq!(result.model_scores[2].model_name, "rules");
    }

    #[test]
    fn ensemble_low_risk_for_normal_tx() {
        let mut scorer = EnsembleScorer::new(&test_config());
        let fv = make_fv(vec![
            ("value_eth", 0.1),
            ("gas_used", 21000.0),
            ("value_z_score", 0.0),
            ("gas_z_score", 0.0),
            ("nonce", 5.0),
        ]);
        let result = scorer.score(&fv);
        assert!(result.score < 0.65, "normal tx should have low score: {}", result.score);
    }

    #[test]
    fn ensemble_higher_for_suspicious_tx() {
        let mut scorer = EnsembleScorer::new(&test_config());
        let fv = make_fv(vec![
            ("value_eth", 200.0),    // triggers high_value rule
            ("value_z_score", 5.0),  // high z-score
            ("gas_z_score", 3.0),
            ("is_contract_creation", 1.0), // triggers contract_creation
        ]);
        let result = scorer.score(&fv);
        // Should be higher than a normal tx
        assert!(result.score > 0.0);
        assert!(!result.triggered_rules.is_empty());
    }

    #[test]
    fn ensemble_weights_are_applied() {
        let mut scorer = EnsembleScorer::new(&test_config());
        let fv = make_fv(vec![("value_eth", 0.01)]);
        let result = scorer.score(&fv);
        // Check weights are correctly assigned
        assert!((result.model_scores[0].weight - 0.4).abs() < 1e-9);
        assert!((result.model_scores[1].weight - 0.35).abs() < 1e-9);
        assert!((result.model_scores[2].weight - 0.25).abs() < 1e-9);
    }

    #[tokio::test]
    async fn pipeline_stage_scores_and_forwards() {
        let config = test_config();
        let scorer = EnsembleScorer::new(&config);

        let (in_tx, in_rx) = mpsc::channel(16);
        let (out_tx, mut out_rx) = mpsc::channel(16);

        tokio::spawn(async move {
            scorer.run(in_rx, out_tx).await.ok();
        });

        // Send a FeaturesExtracted event
        let fv = FeatureVector {
            tx_hash: "0xpipe".into(),
            block_number: 42,
            features: vec![Feature { name: "value_eth".into(), value: 0.5 }],
            extracted_at: Utc::now(),
        };
        in_tx.send(PipelineEvent::FeaturesExtracted(fv)).await.unwrap();
        drop(in_tx); // close channel

        // Should receive a Scored event
        let event = out_rx.recv().await.unwrap();
        match event {
            PipelineEvent::Scored(fs) => {
                assert_eq!(fs.tx_hash, "0xpipe");
                assert!(fs.score >= 0.0 && fs.score <= 1.0);
            }
            _ => panic!("expected Scored event"),
        }
    }
}
