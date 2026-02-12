use fraud_common::types::{FeatureVector, ModelScore};

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
        let mut triggered = Vec::new();
        let mut total_severity = 0.0;

        for rule in &self.rules {
            if let Some(severity) = rule.check(fv) {
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
    /// Check the feature vector; return `Some(severity)` if triggered.
    fn check(&self, fv: &FeatureVector) -> Option<f64>;
}

fn feature_value(fv: &FeatureVector, name: &str) -> Option<f64> {
    fv.features.iter().find(|f| f.name == name).map(|f| f.value)
}

// ---- Concrete rules ----

struct HighValueRule {
    threshold_eth: f64,
}

impl Rule for HighValueRule {
    fn name(&self) -> &str {
        "high_value_transfer"
    }

    fn check(&self, fv: &FeatureVector) -> Option<f64> {
        let val = feature_value(fv, "value_eth")?;
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

    fn check(&self, fv: &FeatureVector) -> Option<f64> {
        let is_first = feature_value(fv, "is_first_tx")?;
        let val = feature_value(fv, "value_eth")?;
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

    fn check(&self, fv: &FeatureVector) -> Option<f64> {
        let v = feature_value(fv, "is_contract_creation")?;
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

    fn check(&self, fv: &FeatureVector) -> Option<f64> {
        let ratio = feature_value(fv, "sender_fan_out_ratio")?;
        if ratio >= self.threshold {
            Some(0.6)
        } else {
            None
        }
    }
}
