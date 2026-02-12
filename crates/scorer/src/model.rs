use extended_isolation_forest::{Forest, ForestOptions};
use fraud_common::config::IsolationForestConfig;
use fraud_common::types::{FeatureVector, ModelScore};
use tracing::{debug, info};

/// Number of numeric features extracted per transaction for the forest.
const NUM_FEATURES: usize = 12;

/// Feature columns extracted from each `FeatureVector` for the forest.
const FOREST_FEATURES: [&str; NUM_FEATURES] = [
    "value_eth",
    "value_log",
    "value_z_score",
    "gas_used",
    "gas_z_score",
    "gas_price_gwei",
    "input_size",
    "is_contract_call",
    "sender_out_degree",
    "sender_fan_out_ratio",
    "receiver_in_degree",
    "nonce",
];

/// Real Isolation Forest anomaly detector backed by `extended-isolation-forest`.
///
/// The model operates in two phases:
///   1. **Warm-up** – collects feature vectors into a training buffer until
///      `min_training_samples` is reached, returning a fallback heuristic
///      score in the meantime.
///   2. **Trained** – uses the fitted `Forest` for scoring, and
///      periodically re-trains on the most recent buffer window.
pub struct IsolationForestModel {
    config: IsolationForestConfig,
    /// Ring-buffer of recent feature rows.
    buffer: Vec<[f64; NUM_FEATURES]>,
    /// The trained model, `None` during warm-up.
    forest: Option<Forest<f64, NUM_FEATURES>>,
    /// Total number of scored transactions since last training.
    scored_since_train: usize,
}

impl IsolationForestModel {
    pub fn new(config: &IsolationForestConfig) -> Self {
        Self {
            config: config.clone(),
            buffer: Vec::with_capacity(config.min_training_samples),
            forest: None,
            scored_since_train: 0,
        }
    }

    /// Extract the fixed-width numeric row from a `FeatureVector`.
    fn extract_row(fv: &FeatureVector) -> [f64; NUM_FEATURES] {
        let mut row = [0.0_f64; NUM_FEATURES];
        for (i, name) in FOREST_FEATURES.iter().enumerate() {
            row[i] = fv
                .features
                .iter()
                .find(|f| f.name == *name)
                .map(|f| f.value)
                .unwrap_or(0.0);
        }
        row
    }

    /// Train (or re-train) the forest on the current buffer.
    fn train(&mut self) {
        let options = ForestOptions {
            n_trees: self.config.num_trees,
            sample_size: self.config.sample_size.min(self.buffer.len()),
            max_tree_depth: None,
            extension_level: 1,
        };

        match Forest::from_slice(self.buffer.as_slice(), &options) {
            Ok(forest) => {
                info!(
                    samples = self.buffer.len(),
                    trees = self.config.num_trees,
                    "isolation forest trained"
                );
                self.forest = Some(forest);
                self.scored_since_train = 0;
            }
            Err(e) => {
                tracing::warn!("isolation forest training failed: {e:?}");
            }
        }
    }

    /// Score a single feature vector. Returns a value in [0.0, 1.0].
    pub fn score(&mut self, fv: &FeatureVector) -> ModelScore {
        let row = Self::extract_row(fv);

        // Always add to training buffer (ring-buffer capped at 2× sample_size)
        let cap = self.config.sample_size * 2;
        if self.buffer.len() >= cap {
            self.buffer.remove(0);
        }
        self.buffer.push(row);

        // Train on first opportunity, then periodically
        let should_train = (self.forest.is_none()
            && self.buffer.len() >= self.config.min_training_samples)
            || (self.forest.is_some()
                && self.scored_since_train >= self.config.retrain_interval);

        if should_train {
            self.train();
        }

        let score = match &self.forest {
            Some(forest) => {
                // `score()` returns a value in [0, 1] where > 0.5 is
                // anomalous (closer to 1.0 = more anomalous).
                let raw = forest.score(&row);
                raw.clamp(0.0, 1.0)
            }
            None => {
                // Fallback during warm-up: simple z-score average
                Self::fallback_score(fv)
            }
        };

        self.scored_since_train += 1;

        debug!(
            tx = %fv.tx_hash,
            score = score,
            trained = self.forest.is_some(),
            "isolation forest score"
        );

        ModelScore {
            model_name: "isolation_forest".into(),
            score,
            weight: 0.0, // filled by ensemble
        }
    }

    /// Heuristic fallback used before the forest is trained.
    fn fallback_score(fv: &FeatureVector) -> f64 {
        let z_scores: Vec<f64> = fv
            .features
            .iter()
            .filter(|f| f.name.ends_with("z_score"))
            .map(|f| f.value.abs())
            .collect();

        if z_scores.is_empty() {
            return 0.0;
        }
        let avg: f64 = z_scores.iter().sum::<f64>() / z_scores.len() as f64;
        // Sigmoid squash
        1.0 / (1.0 + (-avg + 2.0).exp())
    }
}

/// Statistical outlier model based purely on z-scores.
pub struct StatisticalModel;

impl StatisticalModel {
    pub fn new() -> Self {
        Self
    }

    pub fn score(&self, fv: &FeatureVector) -> ModelScore {
        let z_scores: Vec<f64> = fv
            .features
            .iter()
            .filter(|f| f.name.ends_with("z_score"))
            .map(|f| f.value.abs())
            .collect();

        let max_z = z_scores
            .iter()
            .cloned()
            .fold(0.0_f64, f64::max);

        // Map z-score to [0, 1] with a soft cap at z=5
        let score = (max_z / 5.0).min(1.0);

        ModelScore {
            model_name: "statistical".into(),
            score,
            weight: 0.0,
        }
    }
}

impl Default for StatisticalModel {
    fn default() -> Self {
        Self::new()
    }
}
