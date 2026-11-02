//! Descriptive statistics for benchmark samples.
//!
//! * Percentiles use linear interpolation between closest ranks
//!   (`p = (n − 1) × q`, the same definition as NumPy's default).
//! * Standard deviation is the sample standard deviation (n − 1).
//! * Outliers are flagged with the modified z-score: a sample is an outlier
//!   when `|x − median| > 3.5 × 1.4826 × MAD`. Flagged samples stay in the
//!   data and in every summary; they are reported, never hidden.

use super::models::Summary;

/// Threshold of the modified z-score above which a sample is flagged.
pub const OUTLIER_Z: f64 = 3.5;
/// Scales the MAD to estimate the standard deviation of normal data.
const MAD_SCALE: f64 = 1.4826;
/// Coefficient of variation above which results are called out as unstable.
pub const HIGH_VARIATION_CV_PERCENT: f64 = 5.0;

/// Summarizes finite samples. Non-finite values are ignored; returns `None`
/// when nothing is left.
pub fn summarize(samples: &[f64]) -> Option<Summary> {
    let mut sorted: Vec<f64> = samples.iter().copied().filter(|v| v.is_finite()).collect();
    if sorted.is_empty() {
        return None;
    }
    sorted.sort_by(f64::total_cmp);
    let n = sorted.len();
    let mean = sorted.iter().sum::<f64>() / n as f64;
    let std_dev = if n > 1 {
        let var = sorted.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1) as f64;
        var.sqrt()
    } else {
        0.0
    };
    let cv_percent = if mean.abs() > f64::EPSILON {
        std_dev / mean.abs() * 100.0
    } else {
        0.0
    };
    let median = percentile_sorted(&sorted, 0.5);
    Some(Summary {
        n: u32::try_from(n).unwrap_or(u32::MAX),
        mean,
        median,
        min: sorted[0],
        max: sorted[n - 1],
        std_dev,
        cv_percent,
        p50: median,
        p90: percentile_sorted(&sorted, 0.90),
        p95: percentile_sorted(&sorted, 0.95),
        p99: percentile_sorted(&sorted, 0.99),
    })
}

/// Percentile `q` (0–1) of already sorted, finite data.
pub fn percentile_sorted(sorted: &[f64], q: f64) -> f64 {
    match sorted.len() {
        0 => f64::NAN,
        1 => sorted[0],
        n => {
            let rank = q.clamp(0.0, 1.0) * (n - 1) as f64;
            let lo = rank.floor() as usize;
            let hi = rank.ceil() as usize;
            let frac = rank - lo as f64;
            sorted[lo] + (sorted[hi] - sorted[lo]) * frac
        }
    }
}

/// Percentile `q` (0–1) of unsorted data; non-finite values are ignored.
pub fn percentile(samples: &[f64], q: f64) -> Option<f64> {
    let mut sorted: Vec<f64> = samples.iter().copied().filter(|v| v.is_finite()).collect();
    if sorted.is_empty() {
        return None;
    }
    sorted.sort_by(f64::total_cmp);
    Some(percentile_sorted(&sorted, q))
}

pub fn median(samples: &[f64]) -> Option<f64> {
    percentile(samples, 0.5)
}

/// Indices of samples whose modified z-score exceeds [`OUTLIER_Z`]. With
/// fewer than 4 samples, or zero spread, nothing is flagged.
pub fn outlier_indices(samples: &[f64]) -> Vec<u32> {
    if samples.len() < 4 {
        return Vec::new();
    }
    let Some(med) = median(samples) else {
        return Vec::new();
    };
    let deviations: Vec<f64> = samples.iter().map(|v| (v - med).abs()).collect();
    let Some(mad) = median(&deviations) else {
        return Vec::new();
    };
    let spread = MAD_SCALE * mad;
    if spread <= f64::EPSILON * med.abs().max(1.0) {
        return Vec::new();
    }
    samples
        .iter()
        .enumerate()
        .filter(|(_, v)| v.is_finite() && (*v - med).abs() / spread > OUTLIER_Z)
        .filter_map(|(i, _)| u32::try_from(i).ok())
        .collect()
}

/// Jitter: mean absolute difference between consecutive measurements (the
/// definition used for RTT jitter by most speed tests, cf. RFC 3550's
/// interarrival jitter without its exponential smoothing).
pub fn mean_consecutive_difference(samples: &[f64]) -> Option<f64> {
    if samples.len() < 2 {
        return None;
    }
    let diffs: Vec<f64> = samples.windows(2).map(|w| (w[1] - w[0]).abs()).collect();
    Some(diffs.iter().sum::<f64>() / diffs.len() as f64)
}

/// Geometric mean of strictly positive values; `None` if any value is not
/// positive and finite, or the slice is empty.
pub fn geometric_mean(values: &[f64]) -> Option<f64> {
    if values.is_empty() || values.iter().any(|v| !v.is_finite() || *v <= 0.0) {
        return None;
    }
    let log_sum: f64 = values.iter().map(|v| v.ln()).sum();
    Some((log_sum / values.len() as f64).exp())
}

/// Reduces a long series to at most `max` points by averaging equal-width
/// buckets, preserving order. Used for timelines kept in history.
pub fn downsample(values: &[f64], max: usize) -> Vec<f64> {
    if max == 0 || values.len() <= max {
        return values.to_vec();
    }
    let bucket = values.len().div_ceil(max);
    values
        .chunks(bucket)
        .map(|c| c.iter().sum::<f64>() / c.len() as f64)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn summary_of_known_data() {
        let s = summarize(&[2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0]).unwrap();
        assert_eq!(s.n, 8);
        assert!(close(s.mean, 5.0));
        assert!(close(s.median, 4.5));
        assert!(close(s.min, 2.0));
        assert!(close(s.max, 9.0));
        // Sample variance = 32 / 7.
        assert!(close(s.std_dev, (32.0f64 / 7.0).sqrt()));
        assert!(close(s.cv_percent, s.std_dev / 5.0 * 100.0));
    }

    #[test]
    fn percentiles_interpolate_linearly() {
        let data: Vec<f64> = (1..=10).map(f64::from).collect();
        assert!(close(percentile(&data, 0.0).unwrap(), 1.0));
        assert!(close(percentile(&data, 1.0).unwrap(), 10.0));
        assert!(close(percentile(&data, 0.5).unwrap(), 5.5));
        // rank = 0.9 × 9 = 8.1 → 9 + 0.1 × (10 − 9)
        assert!(close(percentile(&data, 0.9).unwrap(), 9.1));
        assert!(close(percentile(&data, 0.95).unwrap(), 9.55));
    }

    #[test]
    fn single_sample_has_zero_spread() {
        let s = summarize(&[3.0]).unwrap();
        assert!(close(s.std_dev, 0.0));
        assert!(close(s.p99, 3.0));
    }

    #[test]
    fn ignores_non_finite_and_handles_empty() {
        assert!(summarize(&[]).is_none());
        assert!(summarize(&[f64::NAN, f64::INFINITY]).is_none());
        let s = summarize(&[1.0, f64::NAN, 3.0]).unwrap();
        assert_eq!(s.n, 2);
        assert!(close(s.mean, 2.0));
    }

    #[test]
    fn flags_outliers_without_removing_them() {
        let data = [100.0, 101.0, 99.0, 100.5, 99.5, 100.2, 160.0];
        assert_eq!(outlier_indices(&data), vec![6]);
        // Summary still includes the outlier.
        assert!(close(summarize(&data).unwrap().max, 160.0));
    }

    #[test]
    fn no_outliers_for_tight_or_tiny_sets() {
        assert!(outlier_indices(&[5.0, 5.0, 5.0, 5.0, 5.0]).is_empty());
        assert!(outlier_indices(&[1.0, 100.0, 1000.0]).is_empty());
    }

    #[test]
    fn jitter_is_mean_absolute_consecutive_difference() {
        let rtts = [20.0, 22.0, 19.0, 25.0];
        // |2| + |−3| + |6| = 11 / 3
        assert!(close(
            mean_consecutive_difference(&rtts).unwrap(),
            11.0 / 3.0
        ));
        assert!(mean_consecutive_difference(&[20.0]).is_none());
    }

    #[test]
    fn geometric_mean_rejects_non_positive() {
        assert!(close(geometric_mean(&[1.0, 4.0, 16.0]).unwrap(), 4.0));
        assert!(geometric_mean(&[]).is_none());
        assert!(geometric_mean(&[1.0, 0.0]).is_none());
        assert!(geometric_mean(&[1.0, -2.0]).is_none());
    }

    #[test]
    fn downsample_averages_buckets() {
        let v: Vec<f64> = (0..10).map(f64::from).collect();
        assert_eq!(downsample(&v, 5), vec![0.5, 2.5, 4.5, 6.5, 8.5]);
        assert_eq!(downsample(&v, 20), v);
    }
}
