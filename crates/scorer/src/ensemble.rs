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
