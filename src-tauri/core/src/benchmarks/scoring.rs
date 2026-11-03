//! Score calculation, version [`SCORE_VERSION`].
//!
//! Scores are presentation aids; raw measurements are always stored and
//! are the source of truth.
//!
//! * **Metric score** = `1000 × measured / reference` for higher-is-better
//!   metrics and `1000 × reference / measured` for lower-is-better ones. A
//!   machine that exactly matches the reference scores 1000 on that metric.
//! * **Category score** = geometric mean of its metric scores, so a 10 %
//!   change in any one metric moves the category by the same proportion
//!   regardless of the metric's units.
//! * **System score** = geometric mean of the CPU, GPU and Disk category
//!   scores. It is only computed when at least two of the three are present,
//!   and is marked partial when one is missing. Missing categories are never
//!   treated as zero. A geometric mean also stops one very strong component
//!   from fully hiding a weak one.
//! * **Internet** results are deliberately excluded from the system score:
//!   they describe the path to a remote server, not this computer.
//!
//! Reference values are fixed constants per score version: round numbers
//! close to what the development machine (a 2024 mid-range laptop, Intel
//! Core 7 150U with integrated Intel Graphics, 10 cores / 12 threads, on AC
//! power) measured, so a score of
//! about 1000 means "comparable to that laptop". They are arbitrary anchors,
//! not derived from and not comparable with any other benchmark product. Changing any constant or the
//! formula requires bumping [`SCORE_VERSION`].

use super::models::{BenchmarkKind, CategoryResult, CategoryStatus, Metric, SystemScore};
use super::statistics::geometric_mean;

pub const SCORE_VERSION: u32 = 1;

/// Scale of a metric score at the reference value.
pub const REFERENCE_SCORE: f64 = 1_000.0;

/// `(metric id, reference value in the metric's unit)`.
///
/// Only metrics listed here contribute to scores; everything else is shown
/// as a raw measurement only.
pub const REFERENCES: &[(&str, f64)] = &[
    // CPU (Mops/s, Mflop/s, MB/s)
    ("cpu.single.integer", 3_500.0),
    ("cpu.multi.integer", 16_000.0),
    ("cpu.single.float", 12_000.0),
    ("cpu.multi.float", 40_000.0),
    ("cpu.single.sha256", 2_000.0),
    ("cpu.multi.sha256", 12_000.0),
    ("cpu.single.lz4.compress", 600.0),
    ("cpu.multi.lz4.compress", 2_500.0),
    ("cpu.single.lz4.decompress", 2_500.0),
    ("cpu.multi.lz4.decompress", 9_000.0),
    // GPU (GFLOP/s, GB/s)
    ("gpu.compute.fma", 1_000.0),
    ("gpu.memory.copy", 30.0),
    ("gpu.memory.readwrite", 30.0),
    // Disk (MB/s, IOPS)
    ("disk.seq.read", 2_000.0),
    ("disk.seq.write", 1_500.0),
    ("disk.rand.read.qd1", 10_000.0),
    ("disk.rand.write.qd1", 20_000.0),
    ("disk.rand.read.t8", 60_000.0),
    ("disk.rand.write.t8", 50_000.0),
];

/// Kinds that contribute to the system score.
pub const SYSTEM_SCORE_KINDS: [BenchmarkKind; 3] =
    [BenchmarkKind::Cpu, BenchmarkKind::Gpu, BenchmarkKind::Disk];

pub fn reference(metric_id: &str) -> Option<f64> {
    REFERENCES
        .iter()
        .find(|(id, _)| *id == metric_id)
        .map(|(_, v)| *v)
}

/// Score of one metric, or `None` if the metric is unscored or its value is
/// not a positive finite number.
pub fn metric_score(metric: &Metric) -> Option<f64> {
    let reference = reference(&metric.id)?;
    let value = metric.value;
    if !value.is_finite() || value <= 0.0 {
        return None;
    }
    let ratio = if metric.higher_is_better {
        value / reference
    } else {
        reference / value
    };
    Some(REFERENCE_SCORE * ratio)
}

/// Fills in metric scores and the category score.
pub fn score_category(category: &mut CategoryResult) {
    for metric in &mut category.metrics {
        metric.score = metric_score(metric);
    }
    category.score = if category.status == CategoryStatus::Completed {
        let scores: Vec<f64> = category.metrics.iter().filter_map(|m| m.score).collect();
        geometric_mean(&scores)
    } else {
        None
    };
}

pub fn system_score(categories: &[CategoryResult]) -> Option<SystemScore> {
    let mut kinds = Vec::new();
    let mut scores = Vec::new();
    for kind in SYSTEM_SCORE_KINDS {
        if let Some(score) = categories
            .iter()
            .find(|c| c.kind == kind && c.status == CategoryStatus::Completed)
            .and_then(|c| c.score)
        {
            kinds.push(kind);
            scores.push(score);
        }
    }
    if scores.len() < 2 {
        return None;
    }
    Some(SystemScore {
        value: geometric_mean(&scores)?,
        partial: scores.len() < SYSTEM_SCORE_KINDS.len(),
        categories: kinds,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmarks::models::MetricUnit;

    fn metric(id: &str, value: f64, higher_is_better: bool) -> Metric {
        Metric {
            id: id.into(),
            label: id.into(),
            unit: MetricUnit::Mops,
            higher_is_better,
            value,
            summary: None,
            samples: vec![value],
            outliers: Vec::new(),
            score: None,
            note: None,
        }
    }

    fn category(kind: BenchmarkKind, score: Option<f64>) -> CategoryResult {
        CategoryResult {
            score,
            ..CategoryResult::empty(kind, CategoryStatus::Completed)
        }
    }

    #[test]
    fn reference_value_scores_exactly_1000() {
        let r = reference("cpu.single.integer").unwrap();
        assert_eq!(
            metric_score(&metric("cpu.single.integer", r, true)),
            Some(1_000.0)
        );
        assert_eq!(
            metric_score(&metric("cpu.single.integer", r * 2.0, true)),
            Some(2_000.0)
        );
    }

    #[test]
    fn lower_is_better_inverts_ratio() {
        // Twice the reference on a lower-is-better metric halves the score.
        let m = metric("disk.seq.read", 4_000.0, false);
        assert_eq!(metric_score(&m), Some(500.0));
    }

    #[test]
    fn unscored_and_invalid_values_have_no_score() {
        assert_eq!(metric_score(&metric("unknown.metric", 5.0, true)), None);
        assert_eq!(metric_score(&metric("cpu.single.integer", 0.0, true)), None);
        assert_eq!(
            metric_score(&metric("cpu.single.integer", f64::NAN, true)),
            None
        );
    }

    #[test]
    fn category_score_is_geometric_mean() {
        let mut c = CategoryResult {
            metrics: vec![
                metric("cpu.single.integer", 1_750.0, true), // 500
                metric("cpu.multi.integer", 32_000.0, true), // 2000
                metric("not.scored", 1.0, true),
            ],
            ..CategoryResult::empty(BenchmarkKind::Cpu, CategoryStatus::Completed)
        };
        score_category(&mut c);
        assert!((c.score.unwrap() - 1_000.0).abs() < 1e-9);
        assert_eq!(c.metrics[2].score, None);
    }

    #[test]
    fn failed_category_has_no_score() {
        let mut c = CategoryResult {
            metrics: vec![metric("cpu.single.integer", 2_000.0, true)],
            ..CategoryResult::empty(BenchmarkKind::Cpu, CategoryStatus::Failed)
        };
        score_category(&mut c);
        assert_eq!(c.score, None);
    }

    #[test]
    fn system_score_needs_two_categories_and_flags_partial() {
        let cpu = category(BenchmarkKind::Cpu, Some(400.0));
        let gpu = category(BenchmarkKind::Gpu, Some(1_600.0));
        let disk = category(BenchmarkKind::Disk, Some(1_000.0));
        let net = category(BenchmarkKind::Network, Some(10_000.0));

        assert!(system_score(std::slice::from_ref(&cpu)).is_none());

        let partial = system_score(&[cpu.clone(), gpu.clone(), net.clone()]).unwrap();
        assert!(partial.partial);
        assert!((partial.value - 800.0).abs() < 1e-9);
        assert_eq!(
            partial.categories,
            vec![BenchmarkKind::Cpu, BenchmarkKind::Gpu]
        );

        let full = system_score(&[cpu, gpu, disk, net]).unwrap();
        assert!(!full.partial);
        // cbrt(400 × 1600 × 1000) = 861.7…; the network score is ignored.
        assert!((full.value - (400.0f64 * 1_600.0 * 1_000.0).cbrt()).abs() < 1e-9);
    }

    #[test]
    fn skipped_categories_are_not_zero() {
        let cpu = category(BenchmarkKind::Cpu, Some(1_000.0));
        let gpu = CategoryResult::empty(BenchmarkKind::Gpu, CategoryStatus::Unsupported);
        let disk = category(BenchmarkKind::Disk, Some(1_000.0));
        let s = system_score(&[cpu, gpu, disk]).unwrap();
        assert!((s.value - 1_000.0).abs() < 1e-9);
        assert!(s.partial);
    }

    #[test]
    fn reference_ids_are_unique() {
        let mut ids: Vec<&str> = REFERENCES.iter().map(|(id, _)| *id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), REFERENCES.len());
        assert!(REFERENCES.iter().all(|(_, v)| *v > 0.0));
    }
}
