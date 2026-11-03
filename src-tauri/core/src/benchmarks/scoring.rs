//! Scores are derived from raw measurements and never stored on their own,
//! so a revised scoring can be recomputed from old results.
//!
//! `score-v1`: `1000 × throughput / reference` per workload, where the
//! reference is a fixed anchor (below). Single-core and multi-core scores are
//! the geometric mean of their five workloads. There is deliberately no
//! combined CPU score: weighting single against multi would be arbitrary.

use super::model::{
    Comparability, CpuMode, CpuScores, MethodInfo, Stability, WorkloadId, WorkloadResult,
};
use super::stats;

pub const CPU_METHOD_ID: &str = "cpu-method-v1";
pub const SCORE_VERSION: &str = "score-v1";

pub fn cpu_method() -> MethodInfo {
    MethodInfo {
        method_id: CPU_METHOD_ID.into(),
        score_version: SCORE_VERSION.into(),
        // Same deterministic code and constants on every platform.
        comparability: Comparability::CrossPlatform,
    }
}

/// Reference throughputs: the medians of three release-mode Standard runs on
/// the development machine (Intel Core 7 150U, 12 logical CPUs, Debian forky,
/// 2026-11-03), rounded. The anchor is arbitrary: it makes scores comparable
/// between runs of this application, not against other benchmark products.
pub fn reference(id: WorkloadId, mode: CpuMode) -> f64 {
    match (mode, id) {
        (CpuMode::SingleCore, WorkloadId::Integer) => 14_000.0,
        (CpuMode::SingleCore, WorkloadId::FloatingPoint) => 11_000.0,
        (CpuMode::SingleCore, WorkloadId::Hash) => 1_950.0,
        (CpuMode::SingleCore, WorkloadId::Compress) => 14.0,
        (CpuMode::SingleCore, WorkloadId::Decompress) => 415.0,
        (CpuMode::MultiCore, WorkloadId::Integer) => 60_000.0,
        (CpuMode::MultiCore, WorkloadId::FloatingPoint) => 62_000.0,
        (CpuMode::MultiCore, WorkloadId::Hash) => 12_800.0,
        (CpuMode::MultiCore, WorkloadId::Compress) => 57.0,
        (CpuMode::MultiCore, WorkloadId::Decompress) => 2_100.0,
    }
}

const ALL: [WorkloadId; 5] = [
    WorkloadId::Integer,
    WorkloadId::FloatingPoint,
    WorkloadId::Hash,
    WorkloadId::Compress,
    WorkloadId::Decompress,
];

/// Score of one workload, or `None` if its median is not a usable number.
pub fn workload_score(w: &WorkloadResult) -> Option<f64> {
    (w.median.is_finite() && w.median > 0.0).then(|| 1000.0 * w.median / reference(w.id, w.mode))
}

fn mode_score(workloads: &[WorkloadResult], mode: CpuMode) -> Option<u32> {
    let scores: Option<Vec<f64>> = ALL
        .iter()
        .map(|id| {
            workloads
                .iter()
                .find(|w| w.mode == mode && w.id == *id)
                .and_then(workload_score)
        })
        .collect();
    let g = stats::geometric_mean(&scores?)?;
    g.is_finite().then(|| g.round() as u32)
}

/// `None` unless all ten workloads are present and valid: a missing workload
/// is not treated as a zero.
pub fn cpu_scores(workloads: &[WorkloadResult]) -> Option<CpuScores> {
    Some(CpuScores {
        single_core: mode_score(workloads, CpuMode::SingleCore)?,
        multi_core: mode_score(workloads, CpuMode::MultiCore)?,
    })
}

/// How repeatable the run looked. A heuristic from the iteration-to-iteration
/// spread (the typical workload, with a cap for the worst one), outliers and
/// warnings; not a confidence interval.
///
/// The typical spread decides, so one noisy workload out of ten does not
/// condemn the whole run, but a very noisy one still lowers the rating.
pub fn stability(workloads: &[WorkloadResult], warnings: &[String]) -> Stability {
    let cvs: Vec<f64> = workloads.iter().filter_map(|w| w.cv_percent).collect();
    let typical = stats::median(&cvs).unwrap_or(0.0);
    let worst = cvs.iter().copied().fold(0.0_f64, f64::max);
    let outliers: usize = workloads.iter().map(|w| w.outlier_indices.len()).sum();
    let mut level = if typical > 10.0 || worst > 25.0 {
        2
    } else if typical > 4.0 || worst > 10.0 {
        1
    } else {
        0
    };
    if !warnings.is_empty() || outliers > 3 {
        level += 1;
    }
    match level {
        0 => Stability::High,
        1 => Stability::Medium,
        _ => Stability::Low,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(id: WorkloadId, mode: CpuMode, median: f64, cv: f64) -> WorkloadResult {
        WorkloadResult {
            id,
            mode,
            label: String::new(),
            unit: String::new(),
            workers: 1,
            samples: vec![median],
            median,
            mean: median,
            min: median,
            max: median,
            std_dev: None,
            cv_percent: Some(cv),
            p95: None,
            outlier_indices: vec![],
            note: None,
        }
    }

    /// A complete set where every workload runs at `factor` × the reference.
    fn machine(factor: f64, cv: f64) -> Vec<WorkloadResult> {
        [CpuMode::SingleCore, CpuMode::MultiCore]
            .into_iter()
            .flat_map(|mode| {
                ALL.iter()
                    .map(move |id| result(*id, mode, reference(*id, mode) * factor, cv))
            })
            .collect()
    }

    #[test]
    fn reference_machine_scores_1000_and_scores_scale_linearly() {
        // Calculation fixtures, not real products: low, reference and high.
        for (factor, expected) in [(0.25, 250), (1.0, 1000), (3.5, 3500)] {
            let s = cpu_scores(&machine(factor, 1.0)).expect("scores");
            assert_eq!((s.single_core, s.multi_core), (expected, expected));
        }
    }

    #[test]
    fn scoring_is_deterministic() {
        let m = machine(1.37, 2.0);
        assert_eq!(cpu_scores(&m), cpu_scores(&m));
    }

    #[test]
    fn score_is_the_geometric_mean_of_the_workloads() {
        let mut m = machine(1.0, 1.0);
        // One workload twice as fast, one half as fast: product unchanged.
        for w in &mut m {
            if w.mode == CpuMode::SingleCore {
                match w.id {
                    WorkloadId::Integer => w.median *= 2.0,
                    WorkloadId::Hash => w.median *= 0.5,
                    _ => {}
                }
            }
        }
        let s = cpu_scores(&m).unwrap();
        assert_eq!(s.single_core, 1000);
        // One workload twice as fast moves the geometric mean by 2^(1/5).
        let mut m2 = machine(1.0, 1.0);
        m2.iter_mut()
            .find(|w| w.mode == CpuMode::MultiCore && w.id == WorkloadId::Integer)
            .unwrap()
            .median *= 2.0;
        assert_eq!(
            cpu_scores(&m2).unwrap().multi_core,
            (1000.0 * 2f64.powf(0.2)).round() as u32
        );
    }

    #[test]
    fn missing_or_invalid_workloads_give_no_score_instead_of_zero() {
        let mut m = machine(1.0, 1.0);
        m.retain(|w| !(w.mode == CpuMode::MultiCore && w.id == WorkloadId::Hash));
        assert_eq!(cpu_scores(&m), None);
        assert_eq!(cpu_scores(&[]), None);
        let mut bad = machine(1.0, 1.0);
        bad[0].median = f64::NAN;
        assert_eq!(cpu_scores(&bad), None);
        bad[0].median = 0.0;
        assert_eq!(cpu_scores(&bad), None);
    }

    #[test]
    fn stability_follows_spread_and_warnings() {
        let none: Vec<String> = vec![];
        let warn = vec!["background load".to_owned()];
        assert_eq!(stability(&machine(1.0, 2.0), &none), Stability::High);
        assert_eq!(stability(&machine(1.0, 7.0), &none), Stability::Medium);
        assert_eq!(stability(&machine(1.0, 15.0), &none), Stability::Low);
        // A warning costs one level.
        assert_eq!(stability(&machine(1.0, 2.0), &warn), Stability::Medium);
        assert_eq!(stability(&machine(1.0, 7.0), &warn), Stability::Low);
        let mut many = machine(1.0, 1.0);
        many[0].outlier_indices = vec![0, 1, 2, 3];
        assert_eq!(stability(&many, &none), Stability::Medium);
    }

    #[test]
    fn one_noisy_workload_lowers_the_rating_but_does_not_condemn_the_run() {
        let none: Vec<String> = vec![];
        let mut m = machine(1.0, 1.0);
        m[0].cv_percent = Some(14.0);
        assert_eq!(stability(&m, &none), Stability::Medium);
        m[0].cv_percent = Some(40.0);
        assert_eq!(stability(&m, &none), Stability::Low);
        // No spread data at all is not treated as unstable.
        let mut bare = machine(1.0, 1.0);
        for w in &mut bare {
            w.cv_percent = None;
        }
        assert_eq!(stability(&bare, &none), Stability::High);
    }
}
