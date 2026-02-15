use std::collections::VecDeque;

/// Rolling statistics for a numeric series.
///
/// Keeps a fixed-size window and computes mean, standard deviation, z-score,
/// min, and max incrementally.
#[derive(Debug, Clone)]
pub struct RollingStats {
    window: VecDeque<f64>,
    capacity: usize,
    /// Running sum for O(1) mean calculation.
    running_sum: f64,
}

impl RollingStats {
    pub fn new(capacity: usize) -> Self {
        Self {
            window: VecDeque::with_capacity(capacity),
            capacity,
            running_sum: 0.0,
        }
    }

    /// Push a new observation, evicting the oldest if at capacity.
    /// O(1) amortised thanks to VecDeque.
    pub fn push(&mut self, value: f64) {
        if self.window.len() == self.capacity {
            if let Some(old) = self.window.pop_front() {
                self.running_sum -= old;
            }
        }
        self.running_sum += value;
        self.window.push_back(value);
    }

    pub fn mean(&self) -> f64 {
        if self.window.is_empty() {
            return 0.0;
        }
        self.running_sum / self.window.len() as f64
    }

    pub fn std_dev(&self) -> f64 {
        if self.window.len() < 2 {
            return 0.0;
        }
        let mean = self.mean();
        let variance =
            self.window.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (self.window.len() - 1) as f64;
        variance.sqrt()
    }

    /// Z-score of the most recent value relative to the window.
    pub fn z_score(&self) -> f64 {
        let sd = self.std_dev();
        if sd == 0.0 {
            return 0.0;
        }
        let last = self.window.back().copied().unwrap_or(0.0);
        (last - self.mean()) / sd
    }

    pub fn min(&self) -> f64 {
        self.window.iter().copied().fold(f64::INFINITY, f64::min)
    }

    pub fn max(&self) -> f64 {
        self.window
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max)
    }

    pub fn len(&self) -> usize {
        self.window.len()
    }

    pub fn is_empty(&self) -> bool {
        self.window.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rolling_mean() {
        let mut s = RollingStats::new(3);
        s.push(1.0);
        s.push(2.0);
        s.push(3.0);
        assert!((s.mean() - 2.0).abs() < 1e-9);
        s.push(6.0); // evicts 1.0
        assert!((s.mean() - (2.0 + 3.0 + 6.0) / 3.0).abs() < 1e-9);
    }

    #[test]
    fn empty_stats() {
        let s = RollingStats::new(10);
        assert_eq!(s.mean(), 0.0);
        assert_eq!(s.std_dev(), 0.0);
        assert_eq!(s.z_score(), 0.0);
        assert!(s.is_empty());
        assert_eq!(s.len(), 0);
    }

    #[test]
    fn single_element_std_dev_is_zero() {
        let mut s = RollingStats::new(10);
        s.push(42.0);
        assert_eq!(s.std_dev(), 0.0);
        assert_eq!(s.z_score(), 0.0);
        assert_eq!(s.len(), 1);
        assert!(!s.is_empty());
    }

    #[test]
    fn all_same_values_zero_std_dev() {
        let mut s = RollingStats::new(5);
        for _ in 0..5 {
            s.push(7.0);
        }
        assert!((s.mean() - 7.0).abs() < 1e-9);
        assert_eq!(s.std_dev(), 0.0);
        assert_eq!(s.z_score(), 0.0);
    }

    #[test]
    fn std_dev_known_values() {
        let mut s = RollingStats::new(5);
        // Values: 2, 4, 4, 4, 5, 5, 7, 9 → push last 5: 4, 5, 5, 7, 9
        for v in &[4.0, 5.0, 5.0, 7.0, 9.0] {
            s.push(*v);
        }
        let mean = (4.0 + 5.0 + 5.0 + 7.0 + 9.0) / 5.0; // 6.0
        assert!((s.mean() - mean).abs() < 1e-9);
        // sample std dev = sqrt(((4-6)^2 + (5-6)^2 + (5-6)^2 + (7-6)^2 + (9-6)^2) / 4)
        // = sqrt((4+1+1+1+9)/4) = sqrt(16/4) = sqrt(4) = 2.0
        assert!((s.std_dev() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn z_score_positive_for_high_outlier() {
        let mut s = RollingStats::new(10);
        for v in &[1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 100.0] {
            s.push(*v);
        }
        assert!(s.z_score() > 2.0, "z_score should be high for outlier: {}", s.z_score());
    }

    #[test]
    fn min_max() {
        let mut s = RollingStats::new(5);
        s.push(3.0);
        s.push(1.0);
        s.push(5.0);
        assert!((s.min() - 1.0).abs() < 1e-9);
        assert!((s.max() - 5.0).abs() < 1e-9);
    }

    #[test]
    fn eviction_at_capacity() {
        let mut s = RollingStats::new(3);
        s.push(10.0);
        s.push(20.0);
        s.push(30.0);
        assert_eq!(s.len(), 3);
        s.push(40.0); // evicts 10.0
        assert_eq!(s.len(), 3);
        assert!((s.min() - 20.0).abs() < 1e-9);
    }
}
