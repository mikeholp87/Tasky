//! Per-operation latency statistics. Every operation's latency is kept, so
//! the percentiles are exact, never derived from averaged intervals.

use super::super::model::{LatencyBucket, LatencySummary};
use super::super::stats;

/// Upper bounds of the histogram buckets, in microseconds (roughly 1-2-5 steps
/// from 1 us to 1 s). Anything above the last bound lands in an open bucket.
const BOUNDS_US: [f64; 19] = [
    1.0,
    2.0,
    5.0,
    10.0,
    20.0,
    50.0,
    100.0,
    200.0,
    500.0,
    1_000.0,
    2_000.0,
    5_000.0,
    10_000.0,
    20_000.0,
    50_000.0,
    100_000.0,
    200_000.0,
    500_000.0,
    1_000_000.0,
];

/// Records latencies in microseconds.
#[derive(Default)]
pub struct LatencyLog {
    samples: Vec<u32>,
}

impl LatencyLog {
    pub fn with_capacity(n: usize) -> Self {
        Self {
            samples: Vec::with_capacity(n),
        }
    }

    pub fn record(&mut self, d: std::time::Duration) {
        // Saturate rather than wrap: a very long operation is still "long".
        self.samples
            .push(u32::try_from(d.as_micros()).unwrap_or(u32::MAX));
    }

    pub fn extend(&mut self, other: LatencyLog) {
        self.samples.extend(other.samples);
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    pub fn summary(&self) -> Option<LatencySummary> {
        summarize(&self.samples)
    }
}

/// Exact mean and percentiles plus a bucketed histogram; `None` for no data.
pub fn summarize(samples: &[u32]) -> Option<LatencySummary> {
    if samples.is_empty() {
        return None;
    }
    let values: Vec<f64> = samples.iter().map(|s| f64::from(*s)).collect();
    let mut buckets: Vec<LatencyBucket> = BOUNDS_US
        .iter()
        .map(|b| LatencyBucket {
            upper_us: *b,
            count: 0,
        })
        .collect();
    // Open-ended bucket for everything slower than the last bound.
    buckets.push(LatencyBucket {
        upper_us: f64::INFINITY,
        count: 0,
    });
    for v in &values {
        let i = BOUNDS_US
            .iter()
            .position(|b| v < b)
            .unwrap_or(BOUNDS_US.len());
        buckets[i].count += 1;
    }
    // JSON cannot carry infinity; the open bucket is marked with a very large
    // finite bound instead.
    if let Some(last) = buckets.last_mut() {
        last.upper_us = f64::MAX;
    }
    buckets.retain(|b| b.count > 0);
    Some(LatencySummary {
        samples: samples.len() as u64,
        mean_us: stats::mean(&values)?,
        p50_us: stats::percentile(&values, 50.0)?,
        p95_us: stats::percentile(&values, 95.0)?,
        p99_us: stats::percentile(&values, 99.0)?,
        max_us: values.iter().copied().fold(0.0, f64::max),
        histogram: buckets,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn percentiles_are_exact_over_every_sample() {
        let samples: Vec<u32> = (1..=1000).collect();
        let s = summarize(&samples).unwrap();
        assert_eq!(s.samples, 1000);
        assert!((s.mean_us - 500.5).abs() < 1e-9);
        assert!((s.p50_us - 500.5).abs() < 1e-9);
        assert!((s.p95_us - 950.05).abs() < 1e-9);
        assert!((s.p99_us - 990.01).abs() < 1e-9);
        assert_eq!(s.max_us, 1000.0);
    }

    #[test]
    fn a_rare_slow_operation_shows_up_in_p99_and_max_but_not_the_median() {
        // 990 fast reads and 10 slow ones: an average would hide the tail.
        let mut samples = vec![100u32; 990];
        samples.extend(vec![50_000u32; 10]);
        let s = summarize(&samples).unwrap();
        assert_eq!(s.p50_us, 100.0);
        assert!(s.p99_us > 100.0, "{}", s.p99_us);
        assert_eq!(s.max_us, 50_000.0);
        assert!(s.mean_us > 100.0 && s.mean_us < 1_000.0);
    }

    #[test]
    fn the_histogram_accounts_for_every_sample() {
        let samples = [0u32, 1, 4, 9, 100, 999, 5_000, 2_000_000, u32::MAX];
        let s = summarize(&samples).unwrap();
        assert_eq!(
            s.histogram.iter().map(|b| b.count).sum::<u64>(),
            samples.len() as u64
        );
        // Bounds ascend and are finite (JSON-safe).
        assert!(
            s.histogram
                .windows(2)
                .all(|p| p[0].upper_us < p[1].upper_us)
        );
        assert!(s.histogram.iter().all(|b| b.upper_us.is_finite()));
        // The two slowest samples are in the open-ended bucket.
        assert_eq!(s.histogram.last().unwrap().count, 2);
        // A value equal to a bound belongs to the next bucket (bounds are exclusive).
        let edge = summarize(&[10]).unwrap();
        assert_eq!(edge.histogram[0].upper_us, 20.0);
    }

    #[test]
    fn no_samples_gives_no_summary_and_one_sample_works() {
        assert!(summarize(&[]).is_none());
        let one = summarize(&[250]).unwrap();
        assert_eq!((one.p50_us, one.p99_us, one.max_us), (250.0, 250.0, 250.0));
    }

    #[test]
    fn the_log_records_durations_and_saturates_instead_of_wrapping() {
        let mut log = LatencyLog::with_capacity(4);
        log.record(Duration::from_micros(150));
        log.record(Duration::from_secs(10_000));
        assert_eq!(log.len(), 2);
        let s = log.summary().unwrap();
        assert_eq!(s.max_us, f64::from(u32::MAX));
        let mut other = LatencyLog::default();
        other.record(Duration::from_micros(5));
        log.extend(other);
        assert_eq!(log.len(), 3);
    }
}
