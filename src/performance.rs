//! Frame-time statistics with the roadmap's explicit 1% low definition.
#[derive(Debug)]
pub struct FrameStats {
    pub frames: usize,
    pub average_fps: f64,
    pub low_one_percent_fps: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub worst_ms: f64,
    pub hitches_over_50ms: usize,
}
impl FrameStats {
    pub fn from_milliseconds(samples: &[f64]) -> Option<Self> {
        if samples.is_empty() || samples.iter().any(|x| !x.is_finite() || *x <= 0.) {
            return None;
        }
        let mut sorted = samples.to_vec();
        sorted.sort_by(f64::total_cmp);
        let n = sorted.len();
        let percentile = |percent: usize| sorted[(n * percent).div_ceil(100).saturating_sub(1).min(n - 1)];
        let slow = n.div_ceil(100);
        let slow_mean = sorted[n - slow..].iter().sum::<f64>() / slow as f64;
        Some(Self {
            frames: n,
            average_fps: 1000. / (sorted.iter().sum::<f64>() / n as f64),
            low_one_percent_fps: 1000. / slow_mean,
            p50_ms: percentile(50),
            p95_ms: percentile(95),
            p99_ms: percentile(99),
            worst_ms: sorted[n - 1],
            hitches_over_50ms: sorted.iter().filter(|&&x| x > 50.).count(),
        })
    }
    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({"frames":self.frames,"average_fps":self.average_fps,"one_percent_low_fps":self.low_one_percent_fps,"p50_ms":self.p50_ms,"p95_ms":self.p95_ms,"p99_ms":self.p99_ms,"worst_ms":self.worst_ms,"hitches_over_50ms":self.hitches_over_50ms})
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn slowest_one_percent_is_an_average_not_a_percentile() {
        let mut times = vec![2.; 198];
        times.extend([10., 30.]);
        let stats = FrameStats::from_milliseconds(&times).unwrap();
        assert_eq!(stats.low_one_percent_fps, 50.);
        assert_eq!(stats.p99_ms, 2.);
        assert_eq!(stats.worst_ms, 30.);
    }
    #[test]
    fn malformed_samples_do_not_become_performance_claims() {
        for samples in [vec![], vec![0.], vec![-1.], vec![f64::NAN], vec![f64::INFINITY]] {
            assert!(FrameStats::from_milliseconds(&samples).is_none());
        }
    }
}
