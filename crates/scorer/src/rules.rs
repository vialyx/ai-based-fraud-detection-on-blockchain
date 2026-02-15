use fraud_common::types::{FeatureVector, ModelScore};
use std::collections::HashMap;

/// Hard-coded heuristic rules that flag obviously suspicious patterns.
pub struct RuleEngine {
    rules: Vec<Box<dyn Rule + Send + Sync>>,
}

impl RuleEngine {
    pub fn new() -> Self {
        Self {
            rules: vec![
                Box::new(HighValueRule { threshold_eth: 100.0 }),
                Box::new(FirstTxHighValueRule { threshold_eth: 10.0 }),
                Box::new(ContractCreationRule),
                Box::new(HighFanOutRule { threshold: 0.95 }),
            ],
        }
    }

    /// Returns a score in [0, 1] plus the list of triggered rule names.
    pub fn evaluate(&self, fv: &FeatureVector) -> (ModelScore, Vec<String>) {
        // Build an index once for O(1) feature lookups across all rules.
        let index: HashMap<&str, f64> = fv
            .features
            .iter()
            .map(|f| (f.name.as_str(), f.value))
            .collect();

        let mut triggered = Vec::new();
        let mut total_severity = 0.0;

        for rule in &self.rules {
            if let Some(severity) = rule.check(&index) {
                triggered.push(rule.name().to_string());
                total_severity += severity;
            }
        }

        let score = (total_severity / self.rules.len() as f64).min(1.0);

        (
            ModelScore {
                model_name: "rules".into(),
                score,
                weight: 0.0,
            },
            triggered,
        )
    }
}

impl Default for RuleEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Rule trait + implementations
// ---------------------------------------------------------------------------

trait Rule {
    /// Human-readable name for the alert.
    fn name(&self) -> &str;
    /// Check the feature index; return `Some(severity)` if triggered.
    fn check(&self, features: &HashMap<&str, f64>) -> Option<f64>;
}

fn feature_value<'a>(features: &'a HashMap<&str, f64>, name: &str) -> Option<f64> {
    features.get(name).copied()
}

// ---- Concrete rules ----

struct HighValueRule {
    threshold_eth: f64,
}

impl Rule for HighValueRule {
    fn name(&self) -> &str {
        "high_value_transfer"
    }

    fn check(&self, features: &HashMap<&str, f64>) -> Option<f64> {
        let val = feature_value(features, "value_eth")?;
        if val >= self.threshold_eth {
            Some(0.8)
        } else {
            None
        }
    }
}

struct FirstTxHighValueRule {
    threshold_eth: f64,
}

impl Rule for FirstTxHighValueRule {
    fn name(&self) -> &str {
        "first_tx_high_value"
    }

    fn check(&self, features: &HashMap<&str, f64>) -> Option<f64> {
        let is_first = feature_value(features, "is_first_tx")?;
        let val = feature_value(features, "value_eth")?;
        if is_first > 0.5 && val >= self.threshold_eth {
            Some(0.9)
        } else {
            None
        }
    }
}

struct ContractCreationRule;

impl Rule for ContractCreationRule {
    fn name(&self) -> &str {
        "contract_creation"
    }

    fn check(&self, features: &HashMap<&str, f64>) -> Option<f64> {
        let v = feature_value(features, "is_contract_creation")?;
        if v > 0.5 {
            Some(0.3) // informational, not necessarily malicious
        } else {
            None
        }
    }
}

struct HighFanOutRule {
    threshold: f64,
}

impl Rule for HighFanOutRule {
    fn name(&self) -> &str {
        "high_fan_out"
    }

    fn check(&self, features: &HashMap<&str, f64>) -> Option<f64> {
        let ratio = feature_value(features, "sender_fan_out_ratio")?;
        if ratio >= self.threshold {
            Some(0.6)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use fraud_common::types::Feature;

    fn make_fv(features: Vec<(&str, f64)>) -> FeatureVector {
        FeatureVector {
            tx_hash: "0xtest".into(),
            block_number: 1,
            features: features.into_iter().map(|(n, v)| Feature { name: n.into(), value: v }).collect(),
            extracted_at: Utc::now(),
        }
    }

    #[test]
    fn high_value_rule_triggers() {
        let engine = RuleEngine::new();
        let fv = make_fv(vec![("value_eth", 200.0)]);
        let (score, rules) = engine.evaluate(&fv);
        assert!(rules.contains(&"high_value_transfer".to_string()));
        assert!(score.score > 0.0);
    }

    #[test]
    fn high_value_rule_does_not_trigger_below_threshold() {
        let engine = RuleEngine::new();
        let fv = make_fv(vec![("value_eth", 50.0)]);
        let (_score, rules) = engine.evaluate(&fv);
        assert!(!rules.contains(&"high_value_transfer".to_string()));
    }

    #[test]
    fn first_tx_high_value_rule_triggers() {
        let engine = RuleEngine::new();
        let fv = make_fv(vec![
            ("is_first_tx", 1.0),
            ("value_eth", 15.0),
        ]);
        let (_score, rules) = engine.evaluate(&fv);
        assert!(rules.contains(&"first_tx_high_value".to_string()));
    }

    #[test]
    fn first_tx_high_value_not_first_tx() {
        let engine = RuleEngine::new();
        let fv = make_fv(vec![
            ("is_first_tx", 0.0),
            ("value_eth", 15.0),
        ]);
        let (_score, rules) = engine.evaluate(&fv);
        assert!(!rules.contains(&"first_tx_high_value".to_string()));
    }

    #[test]
    fn contract_creation_rule_triggers() {
        let engine = RuleEngine::new();
        let fv = make_fv(vec![("is_contract_creation", 1.0)]);
        let (_score, rules) = engine.evaluate(&fv);
        assert!(rules.contains(&"contract_creation".to_string()));
    }

    #[test]
    fn contract_creation_rule_does_not_trigger() {
        let engine = RuleEngine::new();
        let fv = make_fv(vec![("is_contract_creation", 0.0)]);
        let (_score, rules) = engine.evaluate(&fv);
        assert!(!rules.contains(&"contract_creation".to_string()));
    }

    #[test]
    fn high_fan_out_rule_triggers() {
        let engine = RuleEngine::new();
        let fv = make_fv(vec![("sender_fan_out_ratio", 0.98)]);
        let (_score, rules) = engine.evaluate(&fv);
        assert!(rules.contains(&"high_fan_out".to_string()));
    }

    #[test]
    fn high_fan_out_rule_does_not_trigger_below() {
        let engine = RuleEngine::new();
        let fv = make_fv(vec![("sender_fan_out_ratio", 0.5)]);
        let (_score, rules) = engine.evaluate(&fv);
        assert!(!rules.contains(&"high_fan_out".to_string()));
    }

    #[test]
    fn no_rules_trigger_on_empty_features() {
        let engine = RuleEngine::new();
        let fv = make_fv(vec![]);
        let (score, rules) = engine.evaluate(&fv);
        assert!(rules.is_empty());
        assert_eq!(score.score, 0.0);
    }

    #[test]
    fn multiple_rules_trigger() {
        let engine = RuleEngine::new();
        let fv = make_fv(vec![
            ("value_eth", 200.0),
            ("is_contract_creation", 1.0),
            ("sender_fan_out_ratio", 0.99),
        ]);
        let (score, rules) = engine.evaluate(&fv);
        assert!(rules.len() >= 2);
        assert!(score.score > 0.0);
        // Score capped at 1.0
        assert!(score.score <= 1.0);
    }

    #[test]
    fn rule_score_is_averaged_over_total_rules() {
        let engine = RuleEngine::new();
        // Only high_value triggers (severity 0.8) out of 4 rules
        let fv = make_fv(vec![("value_eth", 200.0)]);
        let (score, _) = engine.evaluate(&fv);
        // Expected: 0.8 / 4 = 0.2
        assert!((score.score - 0.2).abs() < 1e-9);
    }
}
