/// Rolling statistics for a numeric series.
///
/// Keeps a fixed-size window and computes mean, standard deviation, z-score,
/// min, and max incrementally.
#[derive(Debug, Clone)]
pub struct RollingStats {
    window: Vec<f64>,
    capacity: usize,
}

impl RollingStats {
    pub fn new(capacity: usize) -> Self {
        Self {
            window: Vec::with_capacity(capacity),
            capacity,
        }
    }

    /// Push a new observation, evicting the oldest if at capacity.
    pub fn push(&mut self, value: f64) {
        if self.window.len() == self.capacity {
            self.window.remove(0);
        }
        self.window.push(value);
    }

    pub fn mean(&self) -> f64 {
        if self.window.is_empty() {
            return 0.0;
        }
        self.window.iter().sum::<f64>() / self.window.len() as f64
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
        let last = self.window.last().copied().unwrap_or(0.0);
        (last - self.mean()) / sd
    }

    pub fn min(&self) -> f64 {
        self.window.iter().cloned().fold(f64::INFINITY, f64::min)
    }

    pub fn max(&self) -> f64 {
        self.window
            .iter()
            .cloned()
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
}
