use crate::engine::PredictionResult;
use serde::Serialize;

// ---------------------------------------------------------------------------
// Confusion matrix
// ---------------------------------------------------------------------------

/// Confusion matrix for binary classification (fraud vs. legitimate).
#[derive(Debug, Clone, Serialize)]
pub struct ConfusionMatrix {
    pub true_positives: usize,
    pub false_positives: usize,
    pub true_negatives: usize,
    pub false_negatives: usize,
}

impl ConfusionMatrix {
    /// Build a confusion matrix from predictions at the given score threshold.
    ///
    /// A transaction is classified as fraud when `score >= threshold`.
    pub fn from_predictions(predictions: &[PredictionResult], threshold: f64) -> Self {
        let (mut tp, mut fp, mut tn, mut fn_) = (0, 0, 0, 0);

        for pred in predictions {
            let predicted_fraud = pred.score >= threshold;
            match (predicted_fraud, pred.is_fraud) {
                (true, true) => tp += 1,
                (true, false) => fp += 1,
                (false, true) => fn_ += 1,
                (false, false) => tn += 1,
            }
        }

        Self {
            true_positives: tp,
            false_positives: fp,
            true_negatives: tn,
            false_negatives: fn_,
        }
    }

    /// Precision = TP / (TP + FP). Returns 0 when denominator is zero.
    pub fn precision(&self) -> f64 {
        let denom = self.true_positives + self.false_positives;
        if denom == 0 {
            return 0.0;
        }
        self.true_positives as f64 / denom as f64
    }

    /// Recall (sensitivity / TPR) = TP / (TP + FN).
    pub fn recall(&self) -> f64 {
        let denom = self.true_positives + self.false_negatives;
        if denom == 0 {
            return 0.0;
        }
        self.true_positives as f64 / denom as f64
    }

    /// F1 score = harmonic mean of precision and recall.
    pub fn f1_score(&self) -> f64 {
        let p = self.precision();
        let r = self.recall();
        if p + r == 0.0 {
            return 0.0;
        }
        2.0 * p * r / (p + r)
    }

    /// Accuracy = (TP + TN) / total.
    pub fn accuracy(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            return 0.0;
        }
        (self.true_positives + self.true_negatives) as f64 / total as f64
    }

    /// Total number of samples.
    pub fn total(&self) -> usize {
        self.true_positives + self.false_positives + self.true_negatives + self.false_negatives
    }

    /// False-positive rate = FP / (FP + TN).
    pub fn fpr(&self) -> f64 {
        let denom = self.false_positives + self.true_negatives;
        if denom == 0 {
            return 0.0;
        }
        self.false_positives as f64 / denom as f64
    }
}

// ---------------------------------------------------------------------------
// Metrics report at a single threshold
// ---------------------------------------------------------------------------

/// Full metrics report at a single classification threshold.
#[derive(Debug, Clone, Serialize)]
pub struct MetricsReport {
    pub threshold: f64,
    pub precision: f64,
    pub recall: f64,
    pub f1_score: f64,
    pub accuracy: f64,
    pub total_samples: usize,
    pub fraud_count: usize,
    pub legit_count: usize,
    pub confusion: ConfusionMatrix,
}

impl MetricsReport {
    /// Compute a full metrics report from predictions at the given threshold.
    pub fn from_predictions(predictions: &[PredictionResult], threshold: f64) -> Self {
        let confusion = ConfusionMatrix::from_predictions(predictions, threshold);
        let fraud_count = predictions.iter().filter(|p| p.is_fraud).count();
        let legit_count = predictions.len() - fraud_count;

        Self {
            threshold,
            precision: confusion.precision(),
            recall: confusion.recall(),
            f1_score: confusion.f1_score(),
            accuracy: confusion.accuracy(),
            total_samples: predictions.len(),
            fraud_count,
            legit_count,
            confusion,
        }
    }

    /// Pretty-print the report to stdout.
    pub fn print(&self) {
        println!("┌──────────────────────────────────────────────┐");
        println!("│        BACKTEST METRICS REPORT                │");
        println!("├──────────────────────────────────────────────┤");
        println!("│  Threshold:       {:<26.4} │", self.threshold);
        println!("│  Total samples:   {:<26}   │", self.total_samples);
        println!("│  Fraud (actual):  {:<26}   │", self.fraud_count);
        println!("│  Legit (actual):  {:<26}   │", self.legit_count);
        println!("├──────────────────────────────────────────────┤");
        println!("│  Precision:       {:<26.4} │", self.precision);
        println!("│  Recall:          {:<26.4} │", self.recall);
        println!("│  F1 Score:        {:<26.4} │", self.f1_score);
        println!("│  Accuracy:        {:<26.4} │", self.accuracy);
        println!("├──────────────────────────────────────────────┤");
        println!("│  Confusion Matrix:                           │");
        println!(
            "│    TP: {:<8}  FP: {:<8}                 │",
            self.confusion.true_positives, self.confusion.false_positives
        );
        println!(
            "│    FN: {:<8}  TN: {:<8}                 │",
            self.confusion.false_negatives, self.confusion.true_negatives
        );
        println!("└──────────────────────────────────────────────┘");
    }
}

// ---------------------------------------------------------------------------
// Threshold sweep
// ---------------------------------------------------------------------------

/// Results from evaluating predictions across a range of thresholds.
#[derive(Debug, Clone, Serialize)]
pub struct ThresholdSweep {
    /// Per-threshold metrics.
    pub reports: Vec<MetricsReport>,
    /// Threshold that maximises F1.
    pub best_f1_threshold: f64,
    /// Best F1 score achieved.
    pub best_f1_score: f64,
    /// Approximate area under the ROC curve.
    pub roc_auc: f64,
}

impl ThresholdSweep {
    /// Evaluate predictions at evenly-spaced thresholds in \[0, 1\].
    pub fn run(predictions: &[PredictionResult], steps: usize) -> Self {
        let steps = steps.max(2);
        let mut reports = Vec::with_capacity(steps + 1);

        for i in 0..=steps {
            let threshold = i as f64 / steps as f64;
            reports.push(MetricsReport::from_predictions(predictions, threshold));
        }

        let (best_f1_threshold, best_f1_score) = reports
            .iter()
            .max_by(|a, b| {
                a.f1_score
                    .partial_cmp(&b.f1_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|r| (r.threshold, r.f1_score))
            .unwrap_or((0.0, 0.0));

        let roc_auc = Self::compute_roc_auc(predictions, steps);

        Self {
            reports,
            best_f1_threshold,
            best_f1_score,
            roc_auc,
        }
    }

    /// Approximate ROC AUC using the trapezoidal rule.
    fn compute_roc_auc(predictions: &[PredictionResult], steps: usize) -> f64 {
        if predictions.is_empty() {
            return 0.0;
        }

        let steps = steps.max(2);
        let mut points: Vec<(f64, f64)> = Vec::with_capacity(steps + 1);

        // Walk from high threshold (low TPR, low FPR) to low threshold
        for i in (0..=steps).rev() {
            let threshold = i as f64 / steps as f64;
            let cm = ConfusionMatrix::from_predictions(predictions, threshold);
            points.push((cm.fpr(), cm.recall()));
        }

        // Sort by FPR for trapezoidal integration
        points.sort_by(|a, b| {
            a.0.partial_cmp(&b.0)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Trapezoidal rule
        let mut auc = 0.0;
        for i in 1..points.len() {
            let dx = points[i].0 - points[i - 1].0;
            let avg_y = (points[i].1 + points[i - 1].1) / 2.0;
            auc += dx * avg_y;
        }

        auc.clamp(0.0, 1.0)
    }

    /// Pretty-print the sweep results.
    pub fn print(&self) {
        println!();
        println!("┌──────────────────────────────────────────────────────────────┐");
        println!("│              THRESHOLD SWEEP RESULTS                         │");
        println!("├──────────────────────────────────────────────────────────────┤");
        println!(
            "│  Best F1 Threshold: {:<38.4}  │",
            self.best_f1_threshold
        );
        println!(
            "│  Best F1 Score:     {:<38.4}  │",
            self.best_f1_score
        );
        println!(
            "│  ROC AUC:           {:<38.4}  │",
            self.roc_auc
        );
        println!("├──────────┬───────────┬──────────┬──────────┬────────────────┤");
        println!("│ Threshold│ Precision │ Recall   │ F1       │ Accuracy       │");
        println!("├──────────┼───────────┼──────────┼──────────┼────────────────┤");
        for report in &self.reports {
            println!(
                "│ {:<8.4} │ {:<9.4} │ {:<8.4} │ {:<8.4} │ {:<14.4} │",
                report.threshold,
                report.precision,
                report.recall,
                report.f1_score,
                report.accuracy,
            );
        }
        println!("└──────────┴───────────┴──────────┴──────────┴────────────────┘");
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use fraud_common::types::RiskLevel;

    fn pred(score: f64, is_fraud: bool) -> PredictionResult {
        PredictionResult {
            tx_hash: format!("0x{}", (score * 1000.0) as u64),
            score,
            risk_level: if score >= 0.65 {
                RiskLevel::High
            } else {
                RiskLevel::Low
            },
            is_fraud,
            triggered_rules: vec![],
        }
    }

    #[test]
    fn confusion_matrix_basic() {
        // 2 fraud (scores 0.9, 0.7), 3 legit (scores 0.1, 0.2, 0.3)
        let preds = vec![
            pred(0.9, true),
            pred(0.7, true),
            pred(0.1, false),
            pred(0.2, false),
            pred(0.3, false),
        ];

        let cm = ConfusionMatrix::from_predictions(&preds, 0.5);
        assert_eq!(cm.true_positives, 2);
        assert_eq!(cm.false_positives, 0);
        assert_eq!(cm.true_negatives, 3);
        assert_eq!(cm.false_negatives, 0);
        assert_eq!(cm.total(), 5);
    }

    #[test]
    fn precision_recall_f1_known() {
        // TP=2, FP=1, TN=2, FN=1
        let preds = vec![
            pred(0.9, true),  // TP
            pred(0.8, true),  // TP
            pred(0.7, false), // FP
            pred(0.3, true),  // FN
            pred(0.2, false), // TN
            pred(0.1, false), // TN
        ];

        let cm = ConfusionMatrix::from_predictions(&preds, 0.5);
        assert_eq!(cm.true_positives, 2);
        assert_eq!(cm.false_positives, 1);
        assert_eq!(cm.true_negatives, 2);
        assert_eq!(cm.false_negatives, 1);

        // Precision = 2/3
        assert!((cm.precision() - 2.0 / 3.0).abs() < 1e-9);
        // Recall = 2/3
        assert!((cm.recall() - 2.0 / 3.0).abs() < 1e-9);
        // F1 = 2/3
        assert!((cm.f1_score() - 2.0 / 3.0).abs() < 1e-9);
        // Accuracy = 4/6
        assert!((cm.accuracy() - 4.0 / 6.0).abs() < 1e-9);
    }

    #[test]
    fn all_predicted_positive() {
        let preds = vec![
            pred(0.9, true),
            pred(0.8, false),
            pred(0.7, true),
        ];
        // threshold 0.0 → all predicted fraud
        let cm = ConfusionMatrix::from_predictions(&preds, 0.0);
        assert_eq!(cm.true_positives, 2);
        assert_eq!(cm.false_positives, 1);
        assert_eq!(cm.true_negatives, 0);
        assert_eq!(cm.false_negatives, 0);
        // Precision = 2/3, Recall = 1.0
        assert!((cm.recall() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn all_predicted_negative() {
        let preds = vec![
            pred(0.1, true),
            pred(0.2, false),
            pred(0.3, true),
        ];
        // threshold 1.0 → none predicted fraud
        let cm = ConfusionMatrix::from_predictions(&preds, 1.0);
        assert_eq!(cm.true_positives, 0);
        assert_eq!(cm.false_positives, 0);
        assert_eq!(cm.true_negatives, 1);
        assert_eq!(cm.false_negatives, 2);
        assert_eq!(cm.precision(), 0.0);
        assert_eq!(cm.recall(), 0.0);
        assert_eq!(cm.f1_score(), 0.0);
    }

    #[test]
    fn empty_predictions() {
        let cm = ConfusionMatrix::from_predictions(&[], 0.5);
        assert_eq!(cm.total(), 0);
        assert_eq!(cm.precision(), 0.0);
        assert_eq!(cm.recall(), 0.0);
        assert_eq!(cm.f1_score(), 0.0);
        assert_eq!(cm.accuracy(), 0.0);
    }

    #[test]
    fn fpr_calculation() {
        // FP=1, TN=3 → FPR = 1/4
        let preds = vec![
            pred(0.9, true),
            pred(0.8, false), // FP
            pred(0.2, false), // TN
            pred(0.1, false), // TN
            pred(0.0, false), // TN
        ];
        let cm = ConfusionMatrix::from_predictions(&preds, 0.5);
        assert!((cm.fpr() - 0.25).abs() < 1e-9);
    }

    #[test]
    fn metrics_report_from_predictions() {
        let preds = vec![
            pred(0.9, true),
            pred(0.1, false),
        ];
        let report = MetricsReport::from_predictions(&preds, 0.5);
        assert_eq!(report.total_samples, 2);
        assert_eq!(report.fraud_count, 1);
        assert_eq!(report.legit_count, 1);
        assert!((report.precision - 1.0).abs() < 1e-9);
        assert!((report.recall - 1.0).abs() < 1e-9);
        assert!((report.f1_score - 1.0).abs() < 1e-9);
        assert!((report.accuracy - 1.0).abs() < 1e-9);
    }

    #[test]
    fn threshold_sweep_finds_best() {
        let preds = vec![
            pred(0.9, true),
            pred(0.7, true),
            pred(0.4, false),
            pred(0.1, false),
        ];
        let sweep = ThresholdSweep::run(&preds, 10);
        assert!(sweep.best_f1_score > 0.0);
        assert!(sweep.reports.len() == 11); // 0..=10
    }

    #[test]
    fn roc_auc_perfect_separation() {
        // Fraud scores: 0.9, 0.8; Legit scores: 0.1, 0.2
        // Perfect separation → AUC ≈ 1.0
        let preds = vec![
            pred(0.9, true),
            pred(0.8, true),
            pred(0.1, false),
            pred(0.2, false),
        ];
        let sweep = ThresholdSweep::run(&preds, 100);
        assert!(
            sweep.roc_auc > 0.9,
            "expected ROC AUC near 1.0 for perfect separation, got {}",
            sweep.roc_auc
        );
    }

    #[test]
    fn roc_auc_overlapping_scores() {
        // Fraud and legit scores overlap heavily → AUC closer to 0.5
        let preds = vec![
            pred(0.6, true),
            pred(0.5, true),
            pred(0.55, false),
            pred(0.45, false),
        ];
        let sweep = ThresholdSweep::run(&preds, 100);
        // With heavy overlap, AUC should be moderate
        assert!(
            sweep.roc_auc < 0.95,
            "expected moderate AUC with overlap, got {}",
            sweep.roc_auc
        );
    }

    #[test]
    fn threshold_sweep_empty() {
        let sweep = ThresholdSweep::run(&[], 10);
        assert_eq!(sweep.best_f1_score, 0.0);
        assert_eq!(sweep.roc_auc, 0.0);
    }
}
