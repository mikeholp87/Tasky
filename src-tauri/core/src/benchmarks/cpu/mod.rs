//! CPU benchmark: integer, floating-point, SHA-256 and LZ4 workloads, each
//! single-core (one worker thread) and multi-core (one worker per logical
//! processor by default), plus a sustained run in the Extended profile.
//!
//! Order: all single-core tests, then all multi-core tests, then the
//! sustained test, so the hottest phase comes last and cannot affect the
//! lighter measurements.

pub mod kernels;
pub mod runner;

use std::sync::Arc;
use std::time::{Duration, Instant};

use kernels::{
    CompressedBlocks, FloatKernel, IntegerKernel, Kernel, LZ4_INPUT_BYTES, Lz4CompressKernel,
    Lz4DecompressKernel, SHA_INPUT_BYTES, ShaKernel, benchmark_data,
};
use runner::{IterationResult, RunPlan};

use super::models::{
    BenchmarkKind, BenchmarkPhase, BenchmarkProfile, BenchmarkRequest, BenchmarkWarning,
    CategoryResult, CategoryStatus, Detail, Metric, MetricUnit, TimelinePoint, WarningCode,
};
use super::statistics::{median, summarize};
use super::suite::{CategoryBenchmark, RunContext, metric_from_samples, metric_value};
use super::thermal::{ThermalProbe, ThermalTracker};
use crate::errors::AppResult;

/// Sustained-run window length.
pub const SUSTAINED_WINDOW: Duration = Duration::from_secs(5);
/// Default sustained duration (Extended profile).
pub const DEFAULT_SUSTAINED_SECS: u32 = 60;
/// Drop from peak to sustained throughput that is called out.
pub const DECLINE_WARN_PERCENT: f64 = 5.0;
/// Minimum interval between sensor reads during a run.
const SENSOR_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Workload {
    Integer,
    Float,
    Sha256,
    Lz4Compress,
    Lz4Decompress,
}

impl Workload {
    const ALL: [Self; 5] = [
        Self::Integer,
        Self::Float,
        Self::Sha256,
        Self::Lz4Compress,
        Self::Lz4Decompress,
    ];

    const fn key(self) -> &'static str {
        match self {
            Self::Integer => "integer",
            Self::Float => "float",
            Self::Sha256 => "sha256",
            Self::Lz4Compress => "lz4.compress",
            Self::Lz4Decompress => "lz4.decompress",
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Integer => "integer",
            Self::Float => "floating point",
            Self::Sha256 => "SHA-256 throughput",
            Self::Lz4Compress => "LZ4 compression",
            Self::Lz4Decompress => "LZ4 decompression",
        }
    }

    /// Kernel work units are ops, FLOPs or bytes; all metric units are
    /// "millions per second" (MB = 10^6 bytes).
    const fn unit(self) -> MetricUnit {
        match self {
            Self::Integer => MetricUnit::Mops,
            Self::Float => MetricUnit::Mflops,
            _ => MetricUnit::MegabytesPerSec,
        }
    }
}

/// Read-only inputs shared by all workers (generated once per run).
struct SharedData {
    sha: Arc<[u8]>,
    lz4: Arc<[u8]>,
    blocks: Arc<CompressedBlocks>,
}

impl SharedData {
    fn new() -> Self {
        let lz4 = benchmark_data(LZ4_INPUT_BYTES, 0x5EED_0002);
        let blocks = Arc::new(CompressedBlocks::new(&lz4));
        Self {
            sha: benchmark_data(SHA_INPUT_BYTES, 0x5EED_0001).into(),
            lz4: lz4.into(),
            blocks,
        }
    }

    fn kernel(&self, workload: Workload, idx: usize) -> Box<dyn Kernel> {
        let seed = 0x00C0_FFEE ^ idx as u64;
        match workload {
            Workload::Integer => Box::new(IntegerKernel::new(seed)),
            Workload::Float => Box::new(FloatKernel::new(seed)),
            Workload::Sha256 => Box::new(ShaKernel::new(Arc::clone(&self.sha))),
            Workload::Lz4Compress => Box::new(Lz4CompressKernel::new(Arc::clone(&self.lz4))),
            Workload::Lz4Decompress => Box::new(Lz4DecompressKernel::new(Arc::clone(&self.blocks))),
        }
    }
}

/// Duration of one measured iteration.
pub const fn target_iteration(profile: BenchmarkProfile) -> Duration {
    match profile {
        BenchmarkProfile::Quick => Duration::from_millis(200),
        BenchmarkProfile::Standard => Duration::from_millis(300),
        BenchmarkProfile::Extended => Duration::from_millis(400),
    }
}

/// Worker count: the request's override, else every logical processor.
pub fn worker_count(request: &BenchmarkRequest, logical: u32) -> usize {
    let available = std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(logical.max(1));
    request
        .cpu_workers
        .unwrap_or(available)
        .clamp(1, super::MAX_CPU_WORKERS) as usize
}

pub fn sustained_secs(request: &BenchmarkRequest) -> u32 {
    request.sustained_secs.unwrap_or(DEFAULT_SUSTAINED_SECS)
}

pub fn estimate_secs(request: &BenchmarkRequest) -> u32 {
    let p = request.profile;
    let per_subtest = p.warmup_ms() as f64 / 1_000.0
        + f64::from(p.iterations()) * target_iteration(p).as_secs_f64();
    let mut secs = per_subtest * (Workload::ALL.len() * 2) as f64 + 1.0;
    if p == BenchmarkProfile::Extended {
        secs += f64::from(sustained_secs(request)) + 2.0;
    }
    secs.ceil() as u32
}

/// Peak, sustained (median of the final third) and drop in percent.
pub fn sustained_summary(windows: &[f64]) -> Option<(f64, f64, f64)> {
    if windows.len() < 3 {
        return None;
    }
    let peak = windows.iter().copied().fold(f64::MIN, f64::max);
    let tail = &windows[windows.len() - windows.len() / 3..];
    let sustained = median(tail)?;
    let drop = if peak > 0.0 {
        ((peak - sustained) / peak * 100.0).max(0.0)
    } else {
        0.0
    };
    Some((peak, sustained, drop))
}

pub struct CpuBenchmark;

struct Sensors {
    thermal: ThermalProbe,
    tracker: ThermalTracker,
    last_read: Option<Instant>,
}

impl Sensors {
    fn sample(&mut self, ctx: &mut RunContext<'_, '_>, force: bool) {
        if !force
            && self
                .last_read
                .is_some_and(|t| t.elapsed() < SENSOR_INTERVAL)
        {
            return;
        }
        self.last_read = Some(Instant::now());
        let temp = self.thermal.cpu();
        let tick = ctx.probe.latest();
        let freq = tick.as_ref().and_then(|t| t.cpu.frequency_mhz);
        self.tracker.record(temp.as_ref(), freq);
        if let Some(t) = &temp {
            ctx.reporter
                .live("cpu.temperature", f64::from(t.celsius), MetricUnit::Celsius);
        }
        if let Some(f) = freq {
            ctx.reporter
                .live("cpu.frequency", f as f64, MetricUnit::Megahertz);
        }
        if let Some(t) = tick {
            ctx.reporter.live(
                "cpu.usage",
                f64::from(t.cpu.usage_percent),
                MetricUnit::Percent,
            );
        }
    }
}

impl CpuBenchmark {
    #[allow(clippy::too_many_arguments)]
    fn measure(
        ctx: &mut RunContext<'_, '_>,
        sensors: &mut Sensors,
        data: &SharedData,
        workload: Workload,
        workers: usize,
        plan_iterations: u32,
        target: Duration,
        warmup: Duration,
        label: &str,
        series: &str,
        step: (usize, usize),
    ) -> AppResult<Vec<IterationResult>> {
        ctx.reporter.subtest(label);
        ctx.reporter
            .phase(BenchmarkPhase::WarmingUp, format!("Warming up: {label}"));
        let plan = RunPlan {
            workers,
            warmup,
            iterations: plan_iterations,
            target_iteration: target,
        };
        let unit = workload.unit();
        let (index, total) = step;
        let make = |idx: usize| data.kernel(workload, idx);
        let cancel = ctx.cancel;
        runner::run(&plan, &make, cancel, |i, r| {
            if i == 1 {
                ctx.reporter
                    .phase(BenchmarkPhase::Running, label.to_owned());
            }
            let value = r.work_per_sec / 1e6;
            ctx.reporter.iteration(i, plan_iterations);
            ctx.reporter.measurement(label, value, unit);
            ctx.reporter.live(series, value, unit);
            let fraction = (index as f32 + i as f32 / plan_iterations as f32) / total as f32;
            ctx.reporter.progress(fraction, label.to_owned());
            sensors.sample(ctx, false);
        })
    }
}

impl CategoryBenchmark for CpuBenchmark {
    fn kind(&self) -> BenchmarkKind {
        BenchmarkKind::Cpu
    }

    fn run(&mut self, ctx: &mut RunContext<'_, '_>) -> AppResult<CategoryResult> {
        let info = ctx.probe.static_info();
        let profile = ctx.request.profile;
        let workers = worker_count(ctx.request, info.logical_core_count);
        let iterations = profile.iterations();
        let target = target_iteration(profile);
        let warmup = Duration::from_millis(profile.warmup_ms());
        let extended = profile == BenchmarkProfile::Extended;

        ctx.reporter
            .phase(BenchmarkPhase::Preparing, "Preparing CPU benchmark data");
        let data = SharedData::new();
        let mut sensors = Sensors {
            thermal: ThermalProbe::new(),
            tracker: ThermalTracker::default(),
            last_read: None,
        };
        sensors.sample(ctx, true);

        let total_steps = Workload::ALL.len() * 2 + usize::from(extended);
        let mut metrics: Vec<Metric> = Vec::new();
        let mut details = Vec::new();
        let mut step = 0;
        for (mode, threads) in [("single", 1usize), ("multi", workers)] {
            for workload in Workload::ALL {
                let mode_label = if mode == "single" {
                    "Single-core"
                } else {
                    "Multi-core"
                };
                let label = format!("{mode_label} {}", workload.label());
                let id = format!("cpu.{mode}.{}", workload.key());
                let results = Self::measure(
                    ctx,
                    &mut sensors,
                    &data,
                    workload,
                    threads,
                    iterations,
                    target,
                    warmup,
                    &label,
                    &id,
                    (step, total_steps),
                )?;
                step += 1;
                let samples: Vec<f64> = results.iter().map(|r| r.work_per_sec / 1e6).collect();
                metrics.push(metric_from_samples(
                    &id,
                    &label,
                    workload.unit(),
                    true,
                    samples,
                ));
                if mode == "multi"
                    && workload == Workload::Integer
                    && let Some(last) = results.last()
                    && let Some(s) = summarize(&last.per_worker)
                {
                    details.push(Detail::new(
                        "perWorkerSpread",
                        "Per-worker integer throughput",
                        format!(
                            "{:.0}–{:.0} Mops/s (median {:.0})",
                            s.min / 1e6,
                            s.max / 1e6,
                            s.median / 1e6
                        ),
                    ));
                }
            }
        }

        // Scaling = multi-core ÷ single-core throughput (per workload; the
        // headline uses the integer workload).
        let mut scaling_notes = Vec::new();
        let mut scaling_metric = None;
        for workload in Workload::ALL {
            let value = |id: &str| metrics.iter().find(|m| m.id == id).map(|m| m.value);
            let key = workload.key();
            if let (Some(s), Some(m)) = (
                value(&format!("cpu.single.{key}")),
                value(&format!("cpu.multi.{key}")),
            ) && s > 0.0
            {
                scaling_notes.push(format!("{} {:.2}×", workload.label(), m / s));
                if workload == Workload::Integer {
                    let mut scaling = metric_value(
                        "cpu.scaling",
                        "Multi-core scaling",
                        MetricUnit::Ratio,
                        true,
                        m / s,
                    );
                    scaling.note = Some(format!(
                        "Multi-core ÷ single-core integer throughput with {workers} workers \
                         (ideal: {workers}×)."
                    ));
                    scaling_metric = Some(scaling);
                }
            }
        }
        metrics.extend(scaling_metric);
        let mut ratio = metric_value(
            "cpu.lz4.ratio",
            "LZ4 compression ratio",
            MetricUnit::Ratio,
            true,
            data.blocks.ratio(),
        );
        ratio.note = Some("Original size ÷ compressed size of the deterministic test data.".into());
        metrics.push(ratio);

        let mut warnings = Vec::new();
        let mut timeline = Vec::new();
        if extended {
            let secs = sustained_secs(ctx.request);
            let windows = (secs / SUSTAINED_WINDOW.as_secs() as u32).max(3);
            let label = "Sustained multi-core";
            let results = Self::measure(
                ctx,
                &mut sensors,
                &data,
                Workload::Integer,
                workers,
                windows,
                SUSTAINED_WINDOW,
                Duration::from_secs(2),
                label,
                "cpu.sustained",
                (step, total_steps),
            )?;
            let samples: Vec<f64> = results.iter().map(|r| r.work_per_sec / 1e6).collect();
            let mut elapsed = 0u64;
            for (r, v) in results.iter().zip(&samples) {
                elapsed += u64::try_from(r.elapsed.as_millis()).unwrap_or(0);
                timeline.push(TimelinePoint {
                    t_ms: elapsed,
                    series: "cpu.sustained".into(),
                    value: *v,
                });
            }
            if let Some((peak, sustained, drop)) = sustained_summary(&samples) {
                metrics.push(metric_value(
                    "cpu.sustained.peak",
                    "Peak performance",
                    MetricUnit::Mops,
                    true,
                    peak,
                ));
                metrics.push(metric_value(
                    "cpu.sustained.value",
                    "Sustained performance",
                    MetricUnit::Mops,
                    true,
                    sustained,
                ));
                metrics.push(metric_value(
                    "cpu.sustained.drop",
                    "Performance drop",
                    MetricUnit::Percent,
                    false,
                    drop,
                ));
                if drop > DECLINE_WARN_PERCENT {
                    warnings.push(BenchmarkWarning::new(
                        WarningCode::PerformanceDecline,
                        format!(
                            "Sustained performance decreased by {drop:.1}% during the test. \
                             Thermal or power limits may be contributing."
                        ),
                    ));
                }
            }
        }

        sensors.sample(ctx, true);
        let tracker = &sensors.tracker;
        if tracker.near_critical() {
            warnings.push(BenchmarkWarning::new(
                WarningCode::HighTemperature,
                "CPU temperature came within 5 °C of the critical threshold reported by the \
                 operating system.",
            ));
        }

        details.extend([
            Detail::new("hardware", "Processor", info.cpu_brand.trim().to_owned()),
            Detail::new(
                "workers",
                "Worker threads",
                format!("1 (single-core), {workers} (multi-core)"),
            ),
            Detail::new(
                "logicalProcessors",
                "Logical processors",
                info.logical_core_count.to_string(),
            ),
            Detail::new(
                "physicalCores",
                "Physical cores",
                info.physical_core_count
                    .map_or_else(|| "Unknown".into(), |n| n.to_string()),
            ),
            Detail::new(
                "timingMethod",
                "Timing",
                format!(
                    "Monotonic clock; fixed work per iteration; median of {iterations} \
                     iterations after a {:.0} s warmup",
                    warmup.as_secs_f64()
                ),
            ),
            Detail::new("scaling", "Scaling by workload", scaling_notes.join(", ")),
        ]);
        let thermal = tracker.observation();
        if !tracker.has_temperature() {
            details.push(Detail::new(
                "temperature",
                "Temperature",
                "Temperature monitoring unavailable on this system.",
            ));
        }
        if let Some(o) = &thermal
            && let (Some(lo), Some(hi)) = (o.min_frequency_mhz, o.max_frequency_mhz)
        {
            let value = if lo == hi {
                format!(
                    "{lo} MHz throughout (as reported by the operating system; may be a \
                     nominal value rather than the actual boost clock)"
                )
            } else {
                format!("{lo}–{hi} MHz (as reported by the operating system)")
            };
            details.push(Detail::new("frequency", "Observed frequency", value));
        }

        let mut result = CategoryResult::empty(BenchmarkKind::Cpu, CategoryStatus::Completed);
        result.metrics = metrics;
        result.details = details;
        result.warnings = warnings;
        result.timeline = timeline;
        result.thermal = thermal;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sustained_summary_uses_final_third() {
        let windows = [100.0, 98.0, 96.0, 92.0, 90.0, 91.0];
        let (peak, sustained, drop) = sustained_summary(&windows).unwrap();
        assert_eq!(peak, 100.0);
        assert!((sustained - 90.5).abs() < 1e-9);
        assert!((drop - 9.5).abs() < 1e-9);
        assert!(sustained_summary(&[1.0, 2.0]).is_none());
    }

    #[test]
    fn worker_count_respects_override_and_cap() {
        let mut r = crate::benchmarks::coordinator_test_request();
        r.cpu_workers = Some(3);
        assert_eq!(worker_count(&r, 8), 3);
        r.cpu_workers = Some(10_000);
        assert_eq!(
            worker_count(&r, 8),
            crate::benchmarks::MAX_CPU_WORKERS as usize
        );
        r.cpu_workers = None;
        assert!(worker_count(&r, 8) >= 1);
    }

    /// Real hardware run (opt-in, loads every core for ~25 s):
    /// `cargo test --release -p tm-core bench_cpu -- --ignored --nocapture`
    #[test]
    #[ignore = "hardware benchmark; run explicitly"]
    fn bench_cpu_quick_real() {
        use crate::benchmarks::coordinator::BenchmarkCoordinator;
        use crate::benchmarks::history::HistoryStore;
        use crate::benchmarks::mock::MockFactory;
        use crate::benchmarks::models::{BenchmarkEvent, BenchmarkPhase};
        use crate::benchmarks::progress::tests::RecordingSink;
        use crate::models::MonitoringState;
        use crate::monitoring::Engine;

        let engine = Arc::new(Engine::start(MonitoringState::default()).unwrap());
        let dir = crate::benchmarks::history::tests::temp_dir("cpu-real");
        let coordinator = BenchmarkCoordinator::new(
            engine.clone(),
            Arc::new(crate::benchmarks::HardwareFactory),
            Arc::new(MockFactory::new(Duration::from_millis(1))),
            Arc::new(HistoryStore::new(Some(dir.clone()))),
            false,
        );
        let sink = Arc::new(RecordingSink::default());
        let id = coordinator
            .start(crate::benchmarks::coordinator_test_request(), sink.clone())
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(300);
        loop {
            let done = sink.events.lock().unwrap().iter().any(
                |e| matches!(e, BenchmarkEvent::Finished { phase, .. } if phase.is_terminal()),
            );
            if done || Instant::now() > deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        let progress = sink.progress();
        println!("progress messages: {}", progress.len());
        let record = coordinator.history().get(&id).unwrap();
        let cpu = &record.categories[0];
        assert_eq!(cpu.status, CategoryStatus::Completed);
        for m in &cpu.metrics {
            println!(
                "{:28} {:>12.1} {:?}  cv={:>5.2}%  score={:?}",
                m.id,
                m.value,
                m.unit,
                m.summary.as_ref().map_or(0.0, |s| s.cv_percent),
                m.score.map(|s| s.round())
            );
        }
        for d in &cpu.details {
            println!("{}: {}", d.label, d.value);
        }
        println!(
            "category score {:?} confidence {:?}",
            cpu.score, cpu.confidence
        );
        println!("warnings {:?}", cpu.warnings);
        assert!(progress.iter().any(|p| p.phase == BenchmarkPhase::Running));
        engine.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn estimates_grow_with_profile() {
        let mut r = crate::benchmarks::coordinator_test_request();
        r.profile = BenchmarkProfile::Quick;
        let quick = estimate_secs(&r);
        r.profile = BenchmarkProfile::Extended;
        let extended = estimate_secs(&r);
        assert!(quick > 5 && extended > quick + 60);
    }
}
