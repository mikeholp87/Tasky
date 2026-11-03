//! Runs the GPU workloads through a [`GpuExecutor`]. This file has no GPU
//! code: calibration, repetition, statistics, validation bookkeeping,
//! timeouts and cancellation are all testable with a fake executor.

use std::time::{Duration, Instant};

use super::super::cancel::CancelToken;
use super::super::cpu::runner::{Observer, analyze_sustained};
use super::super::model::{
    BenchmarkProfile, BenchmarkState, GpuWorkloadId, GpuWorkloadResult, LiveReading,
    SustainedResult, SustainedWindow, TimingMethod,
};
use super::super::{BenchmarkError, stats};

/// Why a dispatch could not complete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GpuError {
    /// The GPU did not finish within the allowed time.
    Timeout,
    /// The driver reset the device or it disappeared.
    DeviceLost(String),
    OutOfMemory,
    /// The output differed from the CPU reference: the run is not trusted.
    WrongOutput(String),
    Other(String),
}

impl From<GpuError> for BenchmarkError {
    fn from(e: GpuError) -> Self {
        BenchmarkError::Failed(match e {
            GpuError::Timeout => "The GPU did not respond in time, so the test was stopped.".into(),
            GpuError::DeviceLost(why) => {
                format!("The graphics driver reset or lost the GPU ({why}). The test was stopped.")
            }
            GpuError::OutOfMemory => "The GPU ran out of memory.".into(),
            GpuError::WrongOutput(what) => format!(
                "The GPU returned results that do not match the reference ({what}), so the measurement was discarded."
            ),
            GpuError::Other(why) => format!("GPU error: {why}"),
        })
    }
}

/// The few things the runner needs from a device.
pub trait GpuExecutor {
    fn timing_method(&self) -> TimingMethod;
    /// Operations (or bytes) in one dispatch at `size`: known, not estimated.
    fn work(&self, workload: GpuWorkloadId, size: u32) -> f64;
    /// Largest `size` this device can take for `workload`.
    fn max_size(&self, workload: GpuWorkloadId) -> u32;
    /// Runs one dispatch and returns its duration in seconds, measured the
    /// way [`timing_method`](Self::timing_method) says. Fails with
    /// [`GpuError::Timeout`] after `timeout`.
    fn run(
        &mut self,
        workload: GpuWorkloadId,
        size: u32,
        timeout: Duration,
    ) -> Result<f64, GpuError>;
    /// Checks the output of the latest `run` against the CPU reference.
    fn validate(&mut self, workload: GpuWorkloadId, size: u32) -> Result<(), GpuError>;
}

#[derive(Debug, Clone)]
pub struct GpuRunConfig {
    pub warmup_iterations: usize,
    pub iterations: usize,
    /// Calibration aims each dispatch at about this long, long enough to
    /// hide submission overhead and short enough to stay far from driver
    /// watchdogs (which reset a GPU after a second or two).
    ///
    /// 80 ms was kept after trying 200 ms on an integrated GPU: longer
    /// dispatches gave 15-25% lower throughput (the GPU slowed under longer
    /// continuous load) and no lower spread.
    pub target_dispatch: Duration,
    pub per_dispatch_timeout: Duration,
    pub sustained: Option<Duration>,
    pub window: Duration,
    /// Threads for the compute workloads (a power of two).
    pub threads: u32,
    /// Bytes per memory-benchmark buffer.
    pub memory_bytes: u64,
}

impl GpuRunConfig {
    pub fn for_profile(profile: BenchmarkProfile, discrete: bool) -> Self {
        let (iterations, sustained) = match profile {
            BenchmarkProfile::Quick => (9, None),
            BenchmarkProfile::Standard => (15, None),
            BenchmarkProfile::Extended => (25, Some(Duration::from_secs(30))),
        };
        Self {
            // About one second of work before measuring: GPU clocks ramp up
            // slowly (a 3-dispatch warm-up left run-to-run spread above 10%).
            warmup_iterations: 12,
            iterations,
            target_dispatch: Duration::from_millis(80),
            per_dispatch_timeout: Duration::from_secs(10),
            sustained,
            window: Duration::from_secs(1),
            threads: if discrete { 1 << 22 } else { 1 << 20 },
            memory_bytes: if profile == BenchmarkProfile::Quick {
                64 << 20
            } else {
                128 << 20
            },
        }
    }

    /// Tiny settings for tests.
    pub fn test() -> Self {
        Self {
            warmup_iterations: 1,
            iterations: 3,
            target_dispatch: Duration::from_millis(5),
            per_dispatch_timeout: Duration::from_secs(2),
            sustained: None,
            window: Duration::from_millis(20),
            threads: 1 << 10,
            memory_bytes: 1 << 20,
        }
    }
}

#[derive(Debug)]
pub struct GpuRunOutput {
    pub workloads: Vec<GpuWorkloadResult>,
    pub sustained: Option<SustainedResult>,
    pub timing_method: TimingMethod,
}

struct Spec {
    id: GpuWorkloadId,
    label: &'static str,
    unit: &'static str,
    /// Divides operations (or bytes) per second to give the unit.
    per_unit: f64,
}

const SPECS: [Spec; 3] = [
    Spec {
        id: GpuWorkloadId::Integer,
        label: "Integer compute",
        unit: "Gops/s",
        per_unit: 1e9,
    },
    Spec {
        id: GpuWorkloadId::FloatingPoint,
        label: "Floating-point compute",
        unit: "GFLOPS",
        per_unit: 1e9,
    },
    Spec {
        id: GpuWorkloadId::Memory,
        label: "Memory bandwidth",
        unit: "GB/s",
        per_unit: 1e9,
    },
];

/// Finds the `size` whose dispatch takes about `target`. Probes grow by at
/// most 8x and only while a probe is still short, so the largest probe stays
/// near the target and cannot trip a driver watchdog.
pub fn calibrate(
    exec: &mut dyn GpuExecutor,
    workload: GpuWorkloadId,
    cfg: &GpuRunConfig,
    cancel: &CancelToken,
) -> Result<u32, BenchmarkError> {
    let cap = exec.max_size(workload).max(1);
    let target = cfg.target_dispatch.as_secs_f64();
    // The first dispatch of a workload pays one-off costs (shader
    // compilation, pipeline creation, first use of the buffers). Discard it,
    // or those costs would be mistaken for the speed of the work.
    exec.run(workload, 1, cfg.per_dispatch_timeout)?;
    let mut size = 1u32;
    loop {
        if cancel.is_cancelled() {
            return Err(BenchmarkError::Cancelled);
        }
        let secs = exec.run(workload, size, cfg.per_dispatch_timeout)?;
        if !secs.is_finite() || secs < 0.0 {
            return Err(BenchmarkError::Failed(
                "the GPU reported an invalid time".into(),
            ));
        }
        // Long enough to scale from, or already as large as allowed.
        if secs >= target / 16.0 || size >= cap {
            let scaled = if secs > 0.0 {
                f64::from(size) * target / secs
            } else {
                f64::from(cap)
            };
            return Ok((scaled.round() as u64).clamp(1, u64::from(cap)) as u32);
        }
        size = size.saturating_mul(8).min(cap);
    }
}

/// Runs the three workloads, then the sustained phase when configured.
pub fn run_gpu(
    exec: &mut dyn GpuExecutor,
    cfg: &GpuRunConfig,
    cancel: &CancelToken,
    observer: &dyn Observer,
) -> Result<GpuRunOutput, BenchmarkError> {
    let rounds_per = cfg.warmup_iterations + cfg.iterations;
    let sustained_secs = cfg.sustained.map_or(0.0, |d| d.as_secs_f64());
    let total_weight = (SPECS.len() * rounds_per) as f64 + sustained_secs;
    let mut done = 0usize;
    let mut results = Vec::with_capacity(SPECS.len());
    let mut sizes = Vec::with_capacity(SPECS.len());

    for spec in &SPECS {
        observer.stage(
            BenchmarkState::WarmingUp,
            &format!("GPU · {} · calibrating", spec.label),
            (done as f64 / total_weight) as f32,
        );
        let size = calibrate(exec, spec.id, cfg, cancel)?;
        sizes.push(size);
        let work = exec.work(spec.id, size);
        let mut samples = Vec::with_capacity(cfg.iterations);
        let mut dispatch_secs = Vec::with_capacity(cfg.iterations);

        for round in 0..rounds_per {
            if cancel.is_cancelled() {
                return Err(BenchmarkError::Cancelled);
            }
            let warm = round < cfg.warmup_iterations;
            let secs = exec.run(spec.id, size, cfg.per_dispatch_timeout)?;
            if !secs.is_finite() || secs <= 0.0 {
                return Err(BenchmarkError::Failed(
                    "the GPU reported an invalid time".into(),
                ));
            }
            done += 1;
            if !warm {
                let value = work / secs / spec.per_unit;
                samples.push(value);
                dispatch_secs.push(secs);
                observer.live(Some(LiveReading {
                    label: spec.label.into(),
                    value,
                    unit: spec.unit.into(),
                }));
            }
            observer.stage(
                if warm {
                    BenchmarkState::WarmingUp
                } else {
                    BenchmarkState::Running
                },
                &format!("GPU · {}", spec.label),
                (done as f64 / total_weight) as f32,
            );
        }
        // The last dispatch's output is checked; a mismatch discards the run.
        exec.validate(spec.id, size)?;

        let s = stats::summarize(&samples)
            .filter(|s| s.median > 0.0)
            .ok_or_else(|| BenchmarkError::Failed("a measurement was not a valid number".into()))?;
        results.push(GpuWorkloadResult {
            id: spec.id,
            label: spec.label.into(),
            unit: spec.unit.into(),
            samples,
            median: s.median,
            mean: s.mean,
            min: s.min,
            max: s.max,
            std_dev: s.std_dev,
            cv_percent: s.cv_percent,
            p95: s.p95,
            outlier_indices: s.outliers.iter().map(|i| *i as u32).collect(),
            work_per_dispatch: work,
            dispatch_ms: stats::median(&dispatch_secs).unwrap_or(0.0) * 1000.0,
            validated: true,
            note: None,
        });
    }

    let sustained = match cfg.sustained {
        Some(duration) => {
            let size = sizes.get(1).copied().unwrap_or(1);
            Some(run_sustained(
                exec,
                cfg,
                duration,
                size,
                cancel,
                observer,
                done as f64,
                total_weight,
            )?)
        }
        None => None,
    };
    Ok(GpuRunOutput {
        workloads: results,
        sustained,
        timing_method: exec.timing_method(),
    })
}

/// Repeats the floating-point workload for `duration`, reporting throughput
/// in one-second windows (mega-operations per second).
#[allow(clippy::too_many_arguments)]
fn run_sustained(
    exec: &mut dyn GpuExecutor,
    cfg: &GpuRunConfig,
    duration: Duration,
    size: u32,
    cancel: &CancelToken,
    observer: &dyn Observer,
    weight_before: f64,
    total_weight: f64,
) -> Result<SustainedResult, BenchmarkError> {
    let work = exec.work(GpuWorkloadId::FloatingPoint, size);
    let start = Instant::now();
    let mut windows = Vec::new();
    while start.elapsed() < duration {
        let window_start = Instant::now();
        let (mut ops, mut busy) = (0.0, 0.0);
        while window_start.elapsed() < cfg.window {
            if cancel.is_cancelled() {
                return Err(BenchmarkError::Cancelled);
            }
            busy += exec.run(GpuWorkloadId::FloatingPoint, size, cfg.per_dispatch_timeout)?;
            ops += work;
        }
        if busy > 0.0 {
            // Throughput of the device while busy, so idle gaps between
            // dispatches (submission and wake-up) do not count against it.
            let mops = ops / busy / 1e6;
            windows.push(SustainedWindow {
                t_secs: start.elapsed().as_secs_f64(),
                mops,
                // GPU clocks and temperatures are not read: no stable,
                // vendor-neutral source exists, and none is invented.
                frequency_mhz: None,
                temperature_c: None,
            });
            observer.live(Some(LiveReading {
                label: "Sustained · floating point".into(),
                value: mops / 1000.0,
                unit: "GFLOPS".into(),
            }));
        }
        observer.stage(
            BenchmarkState::Running,
            "GPU · sustained load",
            ((weight_before + start.elapsed().as_secs_f64()) / total_weight) as f32,
        );
    }
    exec.validate(GpuWorkloadId::FloatingPoint, size)?;
    analyze_sustained(windows)
        .ok_or_else(|| BenchmarkError::Failed("the sustained test produced no samples".into()))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::Mutex;

    /// A fake device: dispatch time is `size / rate`, with optional faults.
    pub struct Fake {
        pub rate_per_sec: f64,
        pub cap: u32,
        pub timing: TimingMethod,
        pub fail_run_after: Option<usize>,
        pub fail_with: GpuError,
        pub bad_output: Option<GpuWorkloadId>,
        pub runs: usize,
        /// Extra seconds on the very first run (one-off compilation cost).
        pub first_run_extra: f64,
        /// Multiplies the time of every n-th run (to create an outlier).
        pub slow_every: Option<(usize, f64)>,
    }

    impl Default for Fake {
        fn default() -> Self {
            Self {
                rate_per_sec: 1000.0,
                cap: 1_000_000,
                timing: TimingMethod::GpuTimestamps,
                fail_run_after: None,
                fail_with: GpuError::Timeout,
                bad_output: None,
                runs: 0,
                first_run_extra: 0.0,
                slow_every: None,
            }
        }
    }

    impl GpuExecutor for Fake {
        fn timing_method(&self) -> TimingMethod {
            self.timing
        }
        fn work(&self, _: GpuWorkloadId, size: u32) -> f64 {
            f64::from(size) * 1e6
        }
        fn max_size(&self, _: GpuWorkloadId) -> u32 {
            self.cap
        }
        fn run(&mut self, _: GpuWorkloadId, size: u32, _: Duration) -> Result<f64, GpuError> {
            self.runs += 1;
            if self.fail_run_after.is_some_and(|n| self.runs > n) {
                return Err(self.fail_with.clone());
            }
            let mut t = f64::from(size) / self.rate_per_sec;
            if self.runs == 1 {
                t += self.first_run_extra;
            }
            if let Some((n, factor)) = self.slow_every
                && self.runs.is_multiple_of(n)
            {
                t *= factor;
            }
            Ok(t)
        }
        fn validate(&mut self, workload: GpuWorkloadId, _: u32) -> Result<(), GpuError> {
            if self.bad_output == Some(workload) {
                Err(GpuError::WrongOutput("sample 7".into()))
            } else {
                Ok(())
            }
        }
    }

    #[derive(Default)]
    struct Rec {
        stages: Mutex<Vec<(BenchmarkState, f32)>>,
        lives: Mutex<usize>,
    }
    impl Observer for Rec {
        fn stage(&self, s: BenchmarkState, _: &str, f: f32) {
            self.stages.lock().unwrap().push((s, f));
        }
        fn live(&self, r: Option<LiveReading>) {
            if r.is_some() {
                *self.lives.lock().unwrap() += 1;
            }
        }
    }

    fn cfg() -> GpuRunConfig {
        GpuRunConfig::test()
    }

    #[test]
    fn calibration_lands_near_the_target_time() {
        let mut f = Fake::default(); // 1000 size-units per second
        let size = calibrate(&mut f, GpuWorkloadId::Integer, &cfg(), &CancelToken::new()).unwrap();
        // 5 ms target at 1000 units/s is 5 units.
        assert!((4..=6).contains(&size), "{size}");
        // A faster device gets proportionally more work per dispatch.
        let mut fast = Fake {
            rate_per_sec: 1_000_000.0,
            ..Fake::default()
        };
        let big = calibrate(
            &mut fast,
            GpuWorkloadId::Integer,
            &cfg(),
            &CancelToken::new(),
        )
        .unwrap();
        assert!((4_000..=6_000).contains(&big), "{big}");
    }

    #[test]
    fn calibration_ignores_the_one_off_cost_of_the_first_dispatch() {
        // Without the throwaway dispatch, a 10 s compile on the first run
        // would make the device look so slow that size 1 was chosen.
        let mut f = Fake {
            first_run_extra: 10.0,
            ..Fake::default()
        };
        let size = calibrate(&mut f, GpuWorkloadId::Integer, &cfg(), &CancelToken::new()).unwrap();
        assert!((4..=6).contains(&size), "{size}");
    }

    #[test]
    fn calibration_never_exceeds_the_device_limit_or_grows_probes_recklessly() {
        let mut f = Fake {
            rate_per_sec: 1e12,
            cap: 777,
            ..Fake::default()
        };
        let size = calibrate(&mut f, GpuWorkloadId::Memory, &cfg(), &CancelToken::new()).unwrap();
        assert_eq!(size, 777);
        // Each probe is at most 8x the last, so a long dispatch is never the
        // first sign that a size is too big (one extra run is the discarded
        // first dispatch).
        assert!(f.runs <= 7, "{} probes", f.runs);
    }

    #[test]
    fn a_full_run_reports_three_validated_workloads_with_raw_samples() {
        let mut f = Fake::default();
        let rec = Rec::default();
        let out = run_gpu(&mut f, &cfg(), &CancelToken::new(), &rec).expect("runs");
        assert_eq!(out.workloads.len(), 3);
        assert_eq!(out.timing_method, TimingMethod::GpuTimestamps);
        for w in &out.workloads {
            assert_eq!(w.samples.len(), cfg().iterations, "{}", w.label);
            assert!(w.validated && w.median > 0.0 && w.work_per_dispatch > 0.0);
            assert!(w.dispatch_ms > 0.0);
        }
        // Unit conversion: 1e6 work per size-unit, 1000 units/s => 1e9 ops/s
        // => 1 in the 1e9-based unit.
        assert!(
            (out.workloads[0].median - 1.0).abs() < 0.2,
            "{}",
            out.workloads[0].median
        );
        let fractions: Vec<f32> = rec.stages.lock().unwrap().iter().map(|s| s.1).collect();
        assert!(fractions.windows(2).all(|p| p[0] <= p[1]));
        assert!(*fractions.last().unwrap() > 0.9);
        assert!(*rec.lives.lock().unwrap() >= 9);
    }

    #[test]
    fn warmup_dispatches_are_not_measured() {
        let mut f = Fake::default();
        let c = cfg();
        run_gpu(&mut f, &c, &CancelToken::new(), &Rec::default()).unwrap();
        // Per workload: at least one probe, then warm-up + iterations runs.
        assert!(f.runs >= 3 * (c.warmup_iterations + c.iterations));
    }

    #[test]
    fn wrong_output_discards_the_run() {
        let mut f = Fake {
            bad_output: Some(GpuWorkloadId::FloatingPoint),
            ..Fake::default()
        };
        let r = run_gpu(&mut f, &cfg(), &CancelToken::new(), &Rec::default());
        assert!(matches!(r, Err(BenchmarkError::Failed(m)) if m.contains("do not match")));
    }

    #[test]
    fn device_faults_become_clear_recoverable_errors() {
        for (err, needle) in [
            (GpuError::Timeout, "did not respond"),
            (GpuError::DeviceLost("hang".into()), "reset or lost"),
            (GpuError::OutOfMemory, "out of memory"),
        ] {
            let mut f = Fake {
                fail_run_after: Some(5),
                fail_with: err,
                ..Fake::default()
            };
            let r = run_gpu(&mut f, &cfg(), &CancelToken::new(), &Rec::default());
            assert!(
                matches!(&r, Err(BenchmarkError::Failed(m)) if m.contains(needle)),
                "{r:?}"
            );
        }
    }

    #[test]
    fn cancellation_stops_between_dispatches() {
        let cancel = CancelToken::new();
        cancel.cancel();
        let r = run_gpu(&mut Fake::default(), &cfg(), &cancel, &Rec::default());
        assert!(matches!(r, Err(BenchmarkError::Cancelled)));
    }

    #[test]
    fn an_invalid_time_is_rejected_not_scored() {
        struct Bad;
        impl GpuExecutor for Bad {
            fn timing_method(&self) -> TimingMethod {
                TimingMethod::WallClock
            }
            fn work(&self, _: GpuWorkloadId, s: u32) -> f64 {
                f64::from(s)
            }
            fn max_size(&self, _: GpuWorkloadId) -> u32 {
                10
            }
            fn run(&mut self, _: GpuWorkloadId, _: u32, _: Duration) -> Result<f64, GpuError> {
                Ok(f64::NAN)
            }
            fn validate(&mut self, _: GpuWorkloadId, _: u32) -> Result<(), GpuError> {
                Ok(())
            }
        }
        let r = run_gpu(&mut Bad, &cfg(), &CancelToken::new(), &Rec::default());
        assert!(matches!(r, Err(BenchmarkError::Failed(m)) if m.contains("invalid time")));
    }

    #[test]
    fn slow_dispatches_are_flagged_as_outliers_not_removed() {
        let mut c = cfg();
        c.iterations = 10;
        let mut f = Fake {
            slow_every: Some((7, 5.0)),
            ..Fake::default()
        };
        let out = run_gpu(&mut f, &c, &CancelToken::new(), &Rec::default()).unwrap();
        let w = &out.workloads[0];
        assert_eq!(w.samples.len(), 10, "nothing removed");
        assert!(!w.outlier_indices.is_empty() || w.cv_percent.unwrap_or(0.0) > 1.0);
    }

    #[test]
    fn the_sustained_phase_reports_windows_and_checks_the_output() {
        let mut c = cfg();
        c.sustained = Some(Duration::from_millis(120));
        let out = run_gpu(
            &mut Fake::default(),
            &c,
            &CancelToken::new(),
            &Rec::default(),
        )
        .unwrap();
        let s = out.sustained.expect("sustained");
        assert!(!s.windows.is_empty());
        assert!(
            s.windows
                .iter()
                .all(|w| w.mops > 0.0 && w.frequency_mhz.is_none() && w.temperature_c.is_none())
        );
    }

    #[test]
    fn profiles_scale_repetitions_not_the_work_definition() {
        let q = GpuRunConfig::for_profile(BenchmarkProfile::Quick, false);
        let s = GpuRunConfig::for_profile(BenchmarkProfile::Standard, false);
        let e = GpuRunConfig::for_profile(BenchmarkProfile::Extended, false);
        assert!(q.iterations < s.iterations && s.iterations < e.iterations);
        assert!(q.sustained.is_none() && s.sustained.is_none() && e.sustained.is_some());
        // The compute workload (threads, target dispatch time) is the same.
        assert_eq!(
            (q.threads, q.target_dispatch),
            (s.threads, s.target_dispatch)
        );
        // Discrete GPUs get more threads to saturate them.
        assert!(GpuRunConfig::for_profile(BenchmarkProfile::Standard, true).threads > s.threads);
    }
}
