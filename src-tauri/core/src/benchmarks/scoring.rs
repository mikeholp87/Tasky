//! Scores are derived from raw measurements and never stored on their own,
//! so a revised scoring can be recomputed from old results.
//!
//! `score-v1`: `1000 × throughput / reference` per workload, where the
//! reference is a fixed anchor (below). Single-core and multi-core scores are
//! the geometric mean of their five workloads. There is deliberately no
//! combined CPU score: weighting single against multi would be arbitrary.

use super::model::{
    Comparability, CpuMode, CpuScores, DiskPhaseKind, DiskPhaseResult, GpuWorkloadId,
    GpuWorkloadResult, IoMode, MethodInfo, Stability, WorkloadId, WorkloadResult,
};
use super::stats;

pub const CPU_METHOD_ID: &str = "cpu-method-v1";
pub const SCORE_VERSION: &str = "score-v1";
pub const GPU_METHOD_ID: &str = "gpu-compute-v1";
pub const GPU_SCORE_VERSION: &str = "gpu-score-v1";

pub fn cpu_method() -> MethodInfo {
    MethodInfo {
        method_id: CPU_METHOD_ID.into(),
        score_version: SCORE_VERSION.into(),
        // Same deterministic code and constants on every platform.
        comparability: Comparability::CrossPlatform,
    }
}

pub const DISK_SCORE_VERSION: &str = "disk-score-v1";
pub const INTERNET_METHOD_ID: &str = "internet-cloudflare-v1";

/// The I/O mode is part of the method: a buffered run measures the page cache
/// as much as the drive, so it must never be compared with a direct one.
pub fn disk_method(mode: IoMode) -> MethodInfo {
    MethodInfo {
        method_id: match mode {
            IoMode::Direct => "disk-direct-v1",
            IoMode::Buffered => "disk-buffered-v1",
        }
        .into(),
        score_version: DISK_SCORE_VERSION.into(),
        // File systems, I/O paths and caching differ between platforms.
        comparability: Comparability::PlatformSpecific,
    }
}

/// Comparable only for the same provider, which the id names. There is no
/// score: the path to the server dominates the result.
pub fn internet_method() -> MethodInfo {
    MethodInfo {
        method_id: INTERNET_METHOD_ID.into(),
        score_version: "none".into(),
        comparability: Comparability::CrossPlatform,
    }
}

/// Disk reference values: medians of repeated direct-I/O release runs on the
/// development machine's NVMe drive (2026-11-03). Sequential phases in MB/s,
/// random phases in operations per second. Arbitrary like the CPU anchor: it
/// compares runs of this application, not other benchmark products.
fn disk_reference(kind: DiskPhaseKind, concurrency: u32) -> Option<f64> {
    Some(match (kind, concurrency) {
        (DiskPhaseKind::SequentialWrite, _) => 5400.0,
        (DiskPhaseKind::SequentialRead, _) => 5100.0,
        (DiskPhaseKind::RandomRead, 1) => 23_000.0,
        (DiskPhaseKind::RandomRead, 8) => 142_000.0,
        (DiskPhaseKind::RandomRead, 32) => 490_000.0,
        (DiskPhaseKind::RandomWrite, 1) => 100_000.0,
        (DiskPhaseKind::RandomWrite, 32) => 1_100_000.0,
        _ => return None,
    })
}

/// Geometric mean of the seven phases against their references, times 1000.
/// Only direct-I/O runs are scored, and only when every phase is present: a
/// partial score would not mean the same thing.
pub fn disk_score(phases: &[DiskPhaseResult], mode: IoMode) -> Option<u32> {
    if mode != IoMode::Direct {
        return None;
    }
    let ratios: Vec<f64> = phases
        .iter()
        .filter_map(|p| {
            let value = if p.kind == DiskPhaseKind::SequentialRead
                || p.kind == DiskPhaseKind::SequentialWrite
            {
                p.median_mbps
            } else {
                p.median_iops
            };
            Some(value / disk_reference(p.kind, p.concurrency)?)
        })
        .collect();
    if ratios.len() != 7 || ratios.iter().any(|r| !r.is_finite() || *r <= 0.0) {
        return None;
    }
    Some((1000.0 * stats::geometric_mean(&ratios)?).round() as u32)
}

/// Throughput samples of both directions; networks are noisier than drives,
/// but the same thresholds keep the rating comparable across benchmarks.
pub fn internet_stability(download: &[f64], upload: &[f64], warnings: &[String]) -> Stability {
    let cvs: Vec<f64> = [download, upload]
        .iter()
        .filter_map(|s| stats::cv_percent(s))
        .collect();
    rate(&cvs, 0, warnings)
}

pub fn disk_stability(phases: &[DiskPhaseResult], warnings: &[String]) -> Stability {
    let cvs: Vec<f64> = phases.iter().filter_map(|p| p.cv_percent).collect();
    let outliers = phases.iter().map(|p| p.outlier_indices.len()).sum();
    rate(&cvs, outliers, warnings)
}

pub fn gpu_method() -> MethodInfo {
    MethodInfo {
        method_id: GPU_METHOD_ID.into(),
        score_version: GPU_SCORE_VERSION.into(),
        // The same WGSL shaders run on every platform. Backend (Vulkan or
        // DirectX 12), driver and timing method are recorded with each result.
        comparability: Comparability::CrossPlatform,
    }
}

/// GPU reference throughputs: rounded medians of repeated release-mode runs
/// on the development machine's integrated GPU (Intel RPL-U graphics, Mesa
/// 26.1.6, Vulkan), 2026-11-03. As with the CPU anchor this is arbitrary: it
/// compares runs of this application, not other benchmark products.
pub fn gpu_reference(id: GpuWorkloadId) -> f64 {
    match id {
        GpuWorkloadId::Integer => 270.0,
        GpuWorkloadId::FloatingPoint => 490.0,
        GpuWorkloadId::Memory => 31.0,
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

/// GPU score: the geometric mean of the three workloads' scores. `None`
/// unless all three are present, validated against the CPU reference, and
/// valid numbers; a missing or unchecked workload is never counted as zero.
pub fn gpu_score(workloads: &[GpuWorkloadResult]) -> Option<u32> {
    const ALL: [GpuWorkloadId; 3] = [
        GpuWorkloadId::Integer,
        GpuWorkloadId::FloatingPoint,
        GpuWorkloadId::Memory,
    ];
    let scores: Option<Vec<f64>> = ALL
        .iter()
        .map(|id| {
            workloads
                .iter()
                .find(|w| w.id == *id && w.validated && w.median.is_finite() && w.median > 0.0)
                .map(|w| 1000.0 * w.median / gpu_reference(*id))
        })
        .collect();
    let g = stats::geometric_mean(&scores?)?;
    g.is_finite().then(|| g.round() as u32)
}

/// Stability of a GPU run; same rules as the CPU.
pub fn gpu_stability(workloads: &[GpuWorkloadResult], warnings: &[String]) -> Stability {
    let cvs: Vec<f64> = workloads.iter().filter_map(|w| w.cv_percent).collect();
    let outliers = workloads.iter().map(|w| w.outlier_indices.len()).sum();
    rate(&cvs, outliers, warnings)
}

/// How repeatable the run looked. A heuristic from the iteration-to-iteration
/// spread (the typical workload, with a cap for the worst one), outliers and
/// warnings; not a confidence interval.
///
/// The typical spread decides, so one noisy workload out of ten does not
/// condemn the whole run, but a very noisy one still lowers the rating.
pub fn stability(workloads: &[WorkloadResult], warnings: &[String]) -> Stability {
    let cvs: Vec<f64> = workloads.iter().filter_map(|w| w.cv_percent).collect();
    let outliers = workloads.iter().map(|w| w.outlier_indices.len()).sum();
    rate(&cvs, outliers, warnings)
}

fn rate(cvs: &[f64], outliers: usize, warnings: &[String]) -> Stability {
    let typical = stats::median(cvs).unwrap_or(0.0);
    let worst = cvs.iter().copied().fold(0.0_f64, f64::max);
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

    fn phase(kind: DiskPhaseKind, qd: u32, mbps: f64, iops: f64) -> DiskPhaseResult {
        DiskPhaseResult {
            kind,
            label: String::new(),
            block_bytes: 4096,
            concurrency: qd,
            queue_depth_exact: true,
            operations: 1,
            bytes: 1,
            seconds: 1.0,
            samples: vec![mbps],
            median_mbps: mbps,
            mean_mbps: mbps,
            min_mbps: mbps,
            max_mbps: mbps,
            cv_percent: Some(1.0),
            outlier_indices: vec![],
            median_iops: iops,
            latency: None,
            validated: true,
            note: None,
        }
    }

    fn reference_phases(scale: f64) -> Vec<DiskPhaseResult> {
        use DiskPhaseKind::*;
        vec![
            phase(SequentialWrite, 1, 5400.0 * scale, 0.0),
            phase(SequentialRead, 1, 5100.0 * scale, 0.0),
            phase(RandomRead, 1, 0.0, 23_000.0 * scale),
            phase(RandomRead, 8, 0.0, 142_000.0 * scale),
            phase(RandomRead, 32, 0.0, 490_000.0 * scale),
            phase(RandomWrite, 1, 0.0, 100_000.0 * scale),
            phase(RandomWrite, 32, 0.0, 1_100_000.0 * scale),
        ]
    }

    #[test]
    fn disk_score_is_1000_at_the_reference_and_scales_linearly() {
        assert_eq!(
            disk_score(&reference_phases(1.0), IoMode::Direct),
            Some(1000)
        );
        assert_eq!(
            disk_score(&reference_phases(0.1), IoMode::Direct),
            Some(100)
        );
    }

    #[test]
    fn buffered_or_incomplete_runs_get_no_disk_score() {
        assert_eq!(disk_score(&reference_phases(1.0), IoMode::Buffered), None);
        let mut p = reference_phases(1.0);
        p.pop();
        assert_eq!(disk_score(&p, IoMode::Direct), None);
        let mut p = reference_phases(1.0);
        p[0].median_mbps = 0.0;
        assert_eq!(disk_score(&p, IoMode::Direct), None);
    }

    #[test]
    fn io_mode_changes_the_method_id() {
        assert_ne!(
            disk_method(IoMode::Direct).method_id,
            disk_method(IoMode::Buffered).method_id
        );
        assert_eq!(internet_method().score_version, "none");
    }

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

    fn gpu(id: GpuWorkloadId, median: f64, validated: bool, cv: f64) -> GpuWorkloadResult {
        GpuWorkloadResult {
            id,
            label: String::new(),
            unit: String::new(),
            samples: vec![median],
            median,
            mean: median,
            min: median,
            max: median,
            std_dev: None,
            cv_percent: Some(cv),
            p95: None,
            outlier_indices: vec![],
            work_per_dispatch: 1.0,
            dispatch_ms: 1.0,
            validated,
            note: None,
        }
    }

    fn gpu_machine(factor: f64) -> Vec<GpuWorkloadResult> {
        [
            GpuWorkloadId::Integer,
            GpuWorkloadId::FloatingPoint,
            GpuWorkloadId::Memory,
        ]
        .iter()
        .map(|id| gpu(*id, gpu_reference(*id) * factor, true, 2.0))
        .collect()
    }

    #[test]
    fn gpu_scores_scale_linearly_from_the_reference() {
        // Calculation fixtures, not real products.
        for (factor, expected) in [(0.1, 100), (1.0, 1000), (8.0, 8000)] {
            assert_eq!(gpu_score(&gpu_machine(factor)), Some(expected));
        }
        assert_eq!(gpu_score(&gpu_machine(1.37)), gpu_score(&gpu_machine(1.37)));
    }

    #[test]
    fn a_gpu_score_needs_every_workload_validated_and_valid() {
        let mut m = gpu_machine(1.0);
        m[1].validated = false;
        assert_eq!(gpu_score(&m), None, "unchecked output is never scored");
        let mut missing = gpu_machine(1.0);
        missing.pop();
        assert_eq!(gpu_score(&missing), None);
        let mut bad = gpu_machine(1.0);
        bad[0].median = f64::NAN;
        assert_eq!(gpu_score(&bad), None);
        bad[0].median = 0.0;
        assert_eq!(gpu_score(&bad), None);
        assert_eq!(gpu_score(&[]), None);
    }

    #[test]
    fn gpu_stability_uses_the_same_rules() {
        let none: Vec<String> = vec![];
        assert_eq!(gpu_stability(&gpu_machine(1.0), &none), Stability::High);
        let noisy: Vec<GpuWorkloadResult> = gpu_machine(1.0)
            .into_iter()
            .map(|mut w| {
                w.cv_percent = Some(15.0);
                w
            })
            .collect();
        assert_eq!(gpu_stability(&noisy, &none), Stability::Low);
        assert_eq!(gpu_method().method_id, "gpu-compute-v1");
    }
}
