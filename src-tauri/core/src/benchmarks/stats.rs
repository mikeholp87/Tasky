//! Statistics over benchmark samples.
//!
//! Every function rejects empty input and non-finite values (`None`) instead
//! of returning NaN, so a bad measurement can never turn into a score.

fn all_finite(v: &[f64]) -> bool {
    !v.is_empty() && v.iter().all(|x| x.is_finite())
}

/// Arithmetic mean. A sum that overflows is rejected rather than returned as
/// infinity.
pub fn mean(v: &[f64]) -> Option<f64> {
    if !all_finite(v) {
        return None;
    }
    let m = v.iter().sum::<f64>() / v.len() as f64;
    m.is_finite().then_some(m)
}

fn sorted(v: &[f64]) -> Option<Vec<f64>> {
    all_finite(v).then(|| {
        let mut s = v.to_vec();
        s.sort_by(f64::total_cmp);
        s
    })
}

pub fn median(v: &[f64]) -> Option<f64> {
    percentile(v, 50.0)
}

/// Sample standard deviation (n − 1). `None` for fewer than two samples.
pub fn std_dev(v: &[f64]) -> Option<f64> {
    if v.len() < 2 {
        return None;
    }
    let m = mean(v)?;
    let var = v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (v.len() - 1) as f64;
    Some(var.sqrt())
}

/// Coefficient of variation in percent (standard deviation / mean × 100).
pub fn cv_percent(v: &[f64]) -> Option<f64> {
    let m = mean(v)?;
    if m == 0.0 {
        return None;
    }
    Some(std_dev(v)? / m.abs() * 100.0)
}

/// Percentile `p` (0..=100) by linear interpolation between closest ranks
/// (the "R-7" definition used by NumPy and Excel).
pub fn percentile(v: &[f64], p: f64) -> Option<f64> {
    if !(0.0..=100.0).contains(&p) {
        return None;
    }
    let s = sorted(v)?;
    let rank = p / 100.0 * (s.len() - 1) as f64;
    let (lo, hi) = (rank.floor() as usize, rank.ceil() as usize);
    let (a, b) = (*s.get(lo)?, *s.get(hi)?);
    Some(a + (b - a) * (rank - lo as f64))
}

/// Indices of samples outside 1.5 × IQR of the quartiles. They are reported,
/// never removed: dropping slow runs silently would flatter the score.
/// Needs four samples; with fewer there is no meaningful quartile.
pub fn iqr_outliers(v: &[f64]) -> Vec<usize> {
    if v.len() < 4 {
        return Vec::new();
    }
    let (Some(q1), Some(q3)) = (percentile(v, 25.0), percentile(v, 75.0)) else {
        return Vec::new();
    };
    let fence = 1.5 * (q3 - q1);
    let (lo, hi) = (q1 - fence, q3 + fence);
    v.iter()
        .enumerate()
        .filter(|(_, x)| **x < lo || **x > hi)
        .map(|(i, _)| i)
        .collect()
}

/// Geometric mean; every value must be positive.
pub fn geometric_mean(v: &[f64]) -> Option<f64> {
    if !all_finite(v) || v.iter().any(|x| *x <= 0.0) {
        return None;
    }
    Some((v.iter().map(|x| x.ln()).sum::<f64>() / v.len() as f64).exp())
}

/// Change from `old` to `new` in percent. `None` when `old` is zero.
pub fn percent_change(old: f64, new: f64) -> Option<f64> {
    (old.is_finite() && new.is_finite() && old != 0.0).then(|| (new - old) / old.abs() * 100.0)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    pub mean: f64,
    pub median: f64,
    pub min: f64,
    pub max: f64,
    pub std_dev: Option<f64>,
    pub cv_percent: Option<f64>,
    pub p95: Option<f64>,
    pub outliers: Vec<usize>,
}

pub fn summarize(v: &[f64]) -> Option<Summary> {
    let s = sorted(v)?;
    Some(Summary {
        mean: mean(v)?,
        median: median(v)?,
        min: *s.first()?,
        max: *s.last()?,
        std_dev: std_dev(v),
        cv_percent: cv_percent(v),
        p95: percentile(v, 95.0),
        outliers: iqr_outliers(v),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Option<f64>, b: f64) {
        let a = a.expect("value");
        assert!((a - b).abs() < 1e-9, "{a} != {b}");
    }

    #[test]
    fn basic_moments() {
        let v = [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
        close(mean(&v), 5.0);
        close(median(&v), 4.5);
        // Sample (n-1) standard deviation of the classic example.
        close(std_dev(&v), (32.0f64 / 7.0).sqrt());
        close(cv_percent(&v), (32.0f64 / 7.0).sqrt() / 5.0 * 100.0);
    }

    #[test]
    fn percentiles_interpolate() {
        let v: Vec<f64> = (1..=100).map(f64::from).collect();
        close(percentile(&v, 0.0), 1.0);
        close(percentile(&v, 50.0), 50.5);
        close(percentile(&v, 95.0), 95.05);
        close(percentile(&v, 99.0), 99.01);
        close(percentile(&v, 100.0), 100.0);
        // Order of input does not matter.
        let mut shuffled = v.clone();
        shuffled.reverse();
        close(percentile(&shuffled, 95.0), 95.05);
        assert_eq!(percentile(&v, 101.0), None);
        assert_eq!(percentile(&v, -1.0), None);
    }

    #[test]
    fn degenerate_inputs_are_none_not_nan() {
        assert_eq!(mean(&[]), None);
        assert_eq!(median(&[]), None);
        assert_eq!(std_dev(&[1.0]), None);
        assert_eq!(mean(&[1.0, f64::NAN]), None);
        assert_eq!(percentile(&[f64::INFINITY], 50.0), None);
        assert_eq!(cv_percent(&[0.0, 0.0]), None);
        assert_eq!(geometric_mean(&[1.0, 0.0]), None);
        assert_eq!(geometric_mean(&[1.0, -2.0]), None);
        assert_eq!(percent_change(0.0, 5.0), None);
        assert!(summarize(&[]).is_none());
    }

    #[test]
    fn single_and_identical_samples() {
        close(median(&[7.0]), 7.0);
        close(percentile(&[7.0], 99.0), 7.0);
        close(std_dev(&[3.0, 3.0, 3.0]), 0.0);
        close(cv_percent(&[3.0, 3.0, 3.0]), 0.0);
        assert!(iqr_outliers(&[3.0, 3.0, 3.0, 3.0]).is_empty());
    }

    #[test]
    fn outliers_are_flagged_not_removed() {
        let v = [100.0, 101.0, 99.0, 100.5, 99.5, 160.0];
        assert_eq!(iqr_outliers(&v), vec![5]);
        let s = summarize(&v).expect("summary");
        assert_eq!(s.outliers, vec![5]);
        assert_eq!(s.max, 160.0);
        // Too few samples for quartiles.
        assert!(iqr_outliers(&[1.0, 2.0, 100.0]).is_empty());
    }

    #[test]
    fn geometric_mean_and_change() {
        close(geometric_mean(&[1.0, 4.0, 16.0]), 4.0);
        close(percent_change(200.0, 250.0), 25.0);
        close(percent_change(200.0, 150.0), -25.0);
    }

    #[test]
    fn large_values_are_handled_without_producing_infinity() {
        close(mean(&[1e300, 1e300]), 1e300);
        // The sum overflows; that must not become an infinite "score".
        assert_eq!(mean(&[f64::MAX, f64::MAX]), None);
        assert_eq!(summarize(&[f64::MAX, f64::MAX]), None);
    }
}
