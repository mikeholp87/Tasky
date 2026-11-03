//! Runs the `cpu-method-v1` workloads: workers, timing, warm-up, iterations
//! and the sustained phase. No OS-specific code: pinning and sensor readings
//! come in through [`CpuHooks`].

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Barrier, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use super::super::BenchmarkError;
use super::super::cancel::CancelToken;
use super::super::model::{
    BenchmarkProfile, BenchmarkState, CpuMode, LiveReading, SustainedResult, SustainedWindow,
    WorkloadId, WorkloadResult,
};
use super::super::stats;
use super::workloads::{self as w, FLOAT_OPS_PER_STEP, INT_OPS_PER_STEP, WorkloadError};

/// Seeds are fixed by the method; chunk `n` uses `BASE_SEED + n`.
const BASE_SEED: u64 = 0x7461_736B_6D67_7231;
const MIB: f64 = 1024.0 * 1024.0;
/// Chunks per single-core pass of the integer, floating-point and hash
/// workloads. Multi-core runs `workers ×` this many, pulled from a shared
/// counter so faster cores (performance cores on hybrid CPUs) take more.
const CHUNKS: usize = 32;
/// The hash input is fixed at 1 MiB.
const HASH_BUF_LEN: usize = 1 << 20;
/// Compression works on 16 slices of the 4 MiB text, one slice per chunk.
const SLICES: usize = 16;
/// Steps per sustained-phase chunk (integer then floating point).
const SUSTAINED_CHUNK_STEPS: u64 = 1 << 16;

/// OS-specific services the runner needs. Everything is optional: a missing
/// capability is recorded, never treated as a failure.
pub trait CpuHooks: Sync {
    /// Pins the calling thread to the `slot`-th usable logical CPU; `false`
    /// if refused.
    fn pin_current_thread(&self, slot: usize) -> bool;
    fn frequency_mhz(&self) -> Option<f64>;
    /// Only when the sensor can be identified as the CPU.
    fn temperature_c(&self) -> Option<f32>;
}

/// Receives progress. Implementations must be cheap and non-blocking.
pub trait Observer: Sync {
    fn stage(&self, state: BenchmarkState, label: &str, fraction: f32);
    fn live(&self, reading: Option<LiveReading>);
}

#[derive(Debug, Clone)]
pub struct CpuRunConfig {
    pub warmup_iterations: usize,
    pub iterations: usize,
    /// Per-chunk sizes (there are [`CHUNKS`] chunks per single-core pass).
    pub int_chunk_steps: u64,
    pub float_chunk_steps: u64,
    pub hash_chunk_rounds: u64,
    /// Passes over the 16 compression slices.
    pub comp_reps: u32,
    pub decomp_reps: u32,
    /// Bytes per compression slice (256 KiB in every real profile).
    pub slice_len: usize,
    pub sustained: Option<Duration>,
    pub window: Duration,
    /// Multi-core worker count (logical CPUs).
    pub workers: usize,
    /// Pin the single-core worker to one logical CPU.
    pub pin_single: bool,
}

impl CpuRunConfig {
    pub fn for_profile(profile: BenchmarkProfile, logical_cpus: usize) -> Self {
        // Profiles change how much work is repeated, never which operations.
        // Throughput divides the size out, so scores stay comparable.
        let (scale, iterations, sustained) = match profile {
            // 0.6 keeps one iteration above ~150 ms: shorter ones are dominated by
            // frequency ramp-up and thread wake-up (0.3 measured too noisy).
            BenchmarkProfile::Quick => (0.6, 3, None),
            BenchmarkProfile::Standard => (1.0, 5, None),
            BenchmarkProfile::Extended => (1.0, 7, Some(Duration::from_secs(30))),
        };
        let per_chunk = |total: f64| ((total * scale / CHUNKS as f64) as u64).max(1);
        Self {
            warmup_iterations: 1,
            iterations,
            int_chunk_steps: per_chunk(128e6),
            float_chunk_steps: per_chunk(128e6),
            hash_chunk_rounds: per_chunk(512.0),
            comp_reps: 1,
            decomp_reps: ((24.0 * scale) as u32).max(1),
            slice_len: 256 << 10,
            sustained,
            window: Duration::from_secs(1),
            workers: logical_cpus.clamp(1, 256),
            pin_single: false,
        }
    }

    /// Tiny workloads for tests: exercises orchestration without load.
    pub fn test() -> Self {
        Self {
            warmup_iterations: 0,
            iterations: 2,
            int_chunk_steps: 600,
            float_chunk_steps: 600,
            hash_chunk_rounds: 1,
            comp_reps: 1,
            decomp_reps: 1,
            slice_len: 4 << 10,
            sustained: None,
            window: Duration::from_millis(50),
            workers: 2,
            pin_single: false,
        }
    }

    fn timed_runs(&self) -> usize {
        self.warmup_iterations + self.iterations
    }
}

pub struct CpuRunOutput {
    pub workloads: Vec<WorkloadResult>,
    pub sustained: Option<SustainedResult>,
    /// What happened with thread pinning, for the result record.
    pub affinity: String,
}

/// One workload definition: what to run and how to express throughput.
struct Spec {
    id: WorkloadId,
    label: &'static str,
    unit: &'static str,
    /// Chunks in one single-core pass.
    chunks: usize,
    /// Throughput units (Mops, MiB) of work in one chunk.
    work_per_chunk: f64,
}

fn specs(cfg: &CpuRunConfig) -> [Spec; 5] {
    let slice_mib = cfg.slice_len as f64 / MIB;
    [
        Spec {
            id: WorkloadId::Integer,
            label: "Integer",
            unit: "Mops/s",
            chunks: CHUNKS,
            work_per_chunk: (cfg.int_chunk_steps * INT_OPS_PER_STEP) as f64 / 1e6,
        },
        Spec {
            id: WorkloadId::FloatingPoint,
            label: "Floating point",
            unit: "Mflops/s",
            chunks: CHUNKS,
            work_per_chunk: (cfg.float_chunk_steps * FLOAT_OPS_PER_STEP) as f64 / 1e6,
        },
        Spec {
            id: WorkloadId::Hash,
            label: "SHA-256 hashing",
            unit: "MiB/s",
            chunks: CHUNKS,
            work_per_chunk: cfg.hash_chunk_rounds as f64 * HASH_BUF_LEN as f64 / MIB,
        },
        Spec {
            id: WorkloadId::Compress,
            label: "Compression (DEFLATE)",
            unit: "MiB/s",
            chunks: SLICES * cfg.comp_reps as usize,
            work_per_chunk: slice_mib,
        },
        Spec {
            id: WorkloadId::Decompress,
            label: "Decompression (DEFLATE)",
            unit: "MiB/s",
            chunks: SLICES * cfg.decomp_reps as usize,
            work_per_chunk: slice_mib,
        },
    ]
}

/// Inputs built once, outside any timing, shared read-only by all workers.
struct Inputs {
    hash_buf: Vec<u8>,
    slices: Vec<Vec<u8>>,
    packed: Vec<Vec<u8>>,
}

impl Inputs {
    fn build(cfg: &CpuRunConfig) -> Result<Self, BenchmarkError> {
        let hash_buf = w::random_bytes(HASH_BUF_LEN, BASE_SEED);
        let text = w::text_buffer(SLICES * cfg.slice_len, BASE_SEED);
        let slices: Vec<Vec<u8>> = text.chunks(cfg.slice_len).map(<[u8]>::to_vec).collect();
        let packed: Vec<Vec<u8>> = slices.iter().map(|s| w::deflate(s)).collect();
        for (s, p) in slices.iter().zip(&packed) {
            w::verify_roundtrip(s, p)
                .map_err(|_| BenchmarkError::Failed("compression self-check failed".into()))?;
        }
        Ok(Self {
            hash_buf,
            slices,
            packed,
        })
    }

    fn ratio(&self) -> f64 {
        let raw: usize = self.slices.iter().map(Vec::len).sum();
        let packed: usize = self.packed.iter().map(Vec::len).sum();
        raw as f64 / packed.max(1) as f64
    }
}

/// Runs `total_chunks` chunks per round on `workers` threads for
/// `warmup + iterations` rounds and returns the wall time of each measured
/// round (barrier release until the last chunk finishes). Workers pull chunk
/// indices from a shared counter, so a faster core simply does more of them.
/// Threads are created once, outside the timing.
#[allow(clippy::too_many_arguments)]
fn measure<F>(
    workers: usize,
    total_chunks: usize,
    pin_slot: Option<usize>,
    hooks: &dyn CpuHooks,
    cfg: &CpuRunConfig,
    cancel: &CancelToken,
    mut on_round: impl FnMut(usize, Duration),
    work: F,
) -> Result<(Vec<Duration>, Option<bool>), BenchmarkError>
where
    F: Fn(usize, &CancelToken) -> Result<u64, WorkloadError> + Sync,
{
    let total = cfg.timed_runs();
    let barrier = Barrier::new(workers + 1);
    let next = AtomicUsize::new(0);
    let panicked = AtomicBool::new(false);
    let invalid: Mutex<Option<&'static str>> = Mutex::new(None);
    // Per worker, per round: XOR of the checksums of the chunks it ran.
    let checks: Mutex<Vec<Vec<u64>>> = Mutex::new(vec![Vec::new(); workers]);
    let pinned: Mutex<Option<bool>> = Mutex::new(None);
    let mut times = Vec::with_capacity(cfg.iterations);

    thread::scope(|scope| {
        for id in 0..workers {
            let (barrier, next, panicked, invalid, checks, pinned, work) = (
                &barrier, &next, &panicked, &invalid, &checks, &pinned, &work,
            );
            scope.spawn(move || {
                if let (0, Some(slot)) = (id, pin_slot) {
                    let ok = hooks.pin_current_thread(slot);
                    *pinned.lock().unwrap_or_else(PoisonError::into_inner) = Some(ok);
                }
                for _ in 0..total {
                    barrier.wait();
                    let mut acc = 0u64;
                    loop {
                        let chunk = next.fetch_add(1, Ordering::Relaxed);
                        if chunk >= total_chunks {
                            break;
                        }
                        // A panic must not skip the barrier or the others
                        // would wait forever.
                        match catch_unwind(AssertUnwindSafe(|| work(chunk, cancel))) {
                            Ok(Ok(sum)) => acc ^= std::hint::black_box(sum),
                            Ok(Err(WorkloadError::Cancelled)) => break,
                            Ok(Err(WorkloadError::Invalid(why))) => {
                                *invalid.lock().unwrap_or_else(PoisonError::into_inner) = Some(why);
                                break;
                            }
                            Err(_) => {
                                panicked.store(true, Ordering::Relaxed);
                                break;
                            }
                        }
                    }
                    checks.lock().unwrap_or_else(PoisonError::into_inner)[id].push(acc);
                    barrier.wait();
                }
            });
        }
        for round in 0..total {
            // Nobody touches the counter between the end barrier and the
            // next start barrier, so the main thread may reset it here.
            next.store(0, Ordering::Relaxed);
            barrier.wait();
            let started = Instant::now();
            barrier.wait();
            let elapsed = started.elapsed();
            if round >= cfg.warmup_iterations {
                times.push(elapsed);
            }
            on_round(round, elapsed);
        }
    });

    if cancel.is_cancelled() {
        return Err(BenchmarkError::Cancelled);
    }
    if panicked.load(Ordering::Relaxed) {
        return Err(BenchmarkError::Failed(
            "a benchmark worker stopped unexpectedly".into(),
        ));
    }
    if let Some(why) = *invalid.lock().unwrap_or_else(PoisonError::into_inner) {
        return Err(BenchmarkError::Failed(format!("invalid result: {why}")));
    }
    // Same chunks every round, so the XOR over all workers must be identical
    // every round no matter which worker ran which chunk.
    let per_worker = checks.lock().unwrap_or_else(PoisonError::into_inner);
    let rounds: Vec<u64> = (0..total)
        .map(|r| {
            per_worker
                .iter()
                .fold(0, |a, w| a ^ w.get(r).copied().unwrap_or(0))
        })
        .collect();
    if per_worker.iter().any(|w| w.len() != total) || rounds.windows(2).any(|p| p[0] != p[1]) {
        return Err(BenchmarkError::Failed(
            "workload output changed between iterations".into(),
        ));
    }
    drop(per_worker);
    let pinned = *pinned.lock().unwrap_or_else(PoisonError::into_inner);
    Ok((times, pinned))
}

fn workload_result(
    spec: &Spec,
    mode: CpuMode,
    workers: usize,
    times: &[Duration],
    note: Option<String>,
) -> Result<WorkloadResult, BenchmarkError> {
    let work = spec.work_per_chunk * (spec.chunks * workers) as f64;
    let samples: Vec<f64> = times.iter().map(|t| work / t.as_secs_f64()).collect();
    let s = stats::summarize(&samples)
        .filter(|s| s.median > 0.0)
        .ok_or_else(|| BenchmarkError::Failed("a measurement was not a valid number".into()))?;
    Ok(WorkloadResult {
        id: spec.id,
        mode,
        label: spec.label.into(),
        unit: spec.unit.into(),
        workers: workers as u32,
        samples,
        median: s.median,
        mean: s.mean,
        min: s.min,
        max: s.max,
        std_dev: s.std_dev,
        cv_percent: s.cv_percent,
        p95: s.p95,
        outlier_indices: s.outliers.iter().map(|i| *i as u32).collect(),
        note,
    })
}

/// Runs all workloads in single- then multi-core mode, then the sustained
/// phase when configured.
pub fn run_cpu(
    cfg: &CpuRunConfig,
    hooks: &dyn CpuHooks,
    cancel: &CancelToken,
    observer: &dyn Observer,
) -> Result<CpuRunOutput, BenchmarkError> {
    // Building inputs is part of preflight; the state only moves forward.
    observer.stage(BenchmarkState::Preflight, "Preparing test data", 0.0);
    let inputs = Inputs::build(cfg)?;
    if cancel.is_cancelled() {
        return Err(BenchmarkError::Cancelled);
    }

    let specs = specs(cfg);
    let sustained_secs = cfg.sustained.map_or(0.0, |d| d.as_secs_f64());
    // Progress weights: each timed round counts 1; sustained seconds count
    // as 1 each, which is roughly their share of wall time.
    let rounds_total = (2 * specs.len() * cfg.timed_runs()) as f64;
    let total_weight = rounds_total + sustained_secs;
    let mut rounds_done = 0usize;
    let mut results = Vec::with_capacity(2 * specs.len());
    let mut affinity = String::from("not pinned");

    for (mode, workers) in [(CpuMode::SingleCore, 1), (CpuMode::MultiCore, cfg.workers)] {
        let mode_label = match mode {
            CpuMode::SingleCore => "Single-core",
            CpuMode::MultiCore => "Multi-core",
        };
        for spec in &specs {
            let label = format!("{mode_label} · {}", spec.label);
            let pin = (mode == CpuMode::SingleCore && cfg.pin_single).then_some(0);
            let state = |round: usize| {
                if round < cfg.warmup_iterations {
                    BenchmarkState::WarmingUp
                } else {
                    BenchmarkState::Running
                }
            };
            observer.stage(state(0), &label, (rounds_done as f64 / total_weight) as f32);
            let total_chunks = spec.chunks * workers;
            let work_total = spec.work_per_chunk * total_chunks as f64;
            let on_round = |round: usize, elapsed: Duration| {
                rounds_done += 1;
                observer.stage(
                    state(round + 1),
                    &label,
                    (rounds_done as f64 / total_weight) as f32,
                );
                if round >= cfg.warmup_iterations {
                    observer.live(Some(LiveReading {
                        label: label.clone(),
                        value: work_total / elapsed.as_secs_f64(),
                        unit: spec.unit.into(),
                    }));
                }
            };
            let mut note = None;
            let run = |on_round,
                       work: &(
                            dyn Fn(usize, &CancelToken) -> Result<u64, WorkloadError> + Sync
                        )| {
                measure(
                    workers,
                    total_chunks,
                    pin,
                    hooks,
                    cfg,
                    cancel,
                    on_round,
                    work,
                )
            };
            let (times, pinned) = match spec.id {
                WorkloadId::Integer => run(on_round, &|i, c| {
                    w::integer(cfg.int_chunk_steps, BASE_SEED + i as u64, c)
                })?,
                WorkloadId::FloatingPoint => run(on_round, &|i, c| {
                    w::floating_point(cfg.float_chunk_steps, BASE_SEED + i as u64, c)
                })?,
                WorkloadId::Hash => run(on_round, &|i, c| {
                    w::hash(
                        cfg.hash_chunk_rounds,
                        &inputs.hash_buf,
                        BASE_SEED + i as u64,
                        c,
                    )
                })?,
                WorkloadId::Compress => {
                    note = Some(format!("compression ratio {:.2}:1", inputs.ratio()));
                    run(on_round, &|i, c| {
                        w::compress_reps(&inputs.slices[i % SLICES], 1, c)
                    })?
                }
                WorkloadId::Decompress => run(on_round, &|i, c| {
                    w::decompress_reps(&inputs.packed[i % SLICES], cfg.slice_len, 1, c)
                })?,
            };
            if let Some(ok) = pinned {
                affinity = if ok {
                    "single-core worker pinned to one logical CPU".into()
                } else {
                    "pinning refused by the system; not pinned".into()
                };
            }
            results.push(workload_result(spec, mode, workers, &times, note)?);
        }
    }

    let sustained = match cfg.sustained {
        Some(duration) => Some(run_sustained(
            cfg,
            duration,
            hooks,
            cancel,
            observer,
            rounds_done as f64,
            total_weight,
        )?),
        None => None,
    };
    Ok(CpuRunOutput {
        workloads: results,
        sustained,
        affinity,
    })
}

/// All workers run a fixed integer+floating-point chunk in a loop; the main
/// thread reads the work counter once per window.
fn run_sustained(
    cfg: &CpuRunConfig,
    duration: Duration,
    hooks: &dyn CpuHooks,
    cancel: &CancelToken,
    observer: &dyn Observer,
    weight_before: f64,
    total_weight: f64,
) -> Result<SustainedResult, BenchmarkError> {
    let ops_per_chunk = SUSTAINED_CHUNK_STEPS * (INT_OPS_PER_STEP + FLOAT_OPS_PER_STEP);
    let stop = AtomicBool::new(false);
    let panicked = AtomicBool::new(false);
    let chunks = AtomicU64::new(0);
    let mut windows = Vec::new();

    thread::scope(|scope| {
        for id in 0..cfg.workers {
            let (stop, panicked, chunks) = (&stop, &panicked, &chunks);
            scope.spawn(move || {
                let mut i = 0u64;
                while !stop.load(Ordering::Relaxed) && !cancel.is_cancelled() {
                    let seed = BASE_SEED + id as u64 + i;
                    let run = catch_unwind(AssertUnwindSafe(|| {
                        let a = w::integer(SUSTAINED_CHUNK_STEPS, seed, cancel)?;
                        let b = w::floating_point(SUSTAINED_CHUNK_STEPS, seed, cancel)?;
                        Ok::<u64, WorkloadError>(std::hint::black_box(a ^ b))
                    }));
                    match run {
                        Ok(Ok(_)) => {
                            chunks.fetch_add(1, Ordering::Relaxed);
                        }
                        Ok(Err(_)) => break,
                        Err(_) => {
                            panicked.store(true, Ordering::Relaxed);
                            break;
                        }
                    }
                    i += 1;
                }
            });
        }

        let start = Instant::now();
        let (mut last_t, mut last_chunks) = (start, 0u64);
        while start.elapsed() < duration && !cancel.is_cancelled() {
            let next = last_t + cfg.window;
            while Instant::now() < next && !cancel.is_cancelled() {
                thread::sleep(Duration::from_millis(20).min(next - Instant::now().min(next)));
            }
            let now = Instant::now();
            let done = chunks.load(Ordering::Relaxed);
            let dt = (now - last_t).as_secs_f64();
            // A window cut short by cancellation is not a full sample.
            if !cancel.is_cancelled() && dt > 0.0 {
                let mops = (done - last_chunks) as f64 * ops_per_chunk as f64 / dt / 1e6;
                windows.push(SustainedWindow {
                    t_secs: (now - start).as_secs_f64(),
                    mops,
                    frequency_mhz: hooks.frequency_mhz(),
                    temperature_c: hooks.temperature_c(),
                });
                observer.live(Some(LiveReading {
                    label: "Sustained · multi-core".into(),
                    value: mops,
                    unit: "Mops/s".into(),
                }));
                observer.stage(
                    BenchmarkState::Running,
                    "Sustained multi-core load",
                    ((weight_before + (now - start).as_secs_f64()) / total_weight) as f32,
                );
            }
            (last_t, last_chunks) = (now, done);
        }
        stop.store(true, Ordering::Relaxed);
    });

    if cancel.is_cancelled() {
        return Err(BenchmarkError::Cancelled);
    }
    if panicked.load(Ordering::Relaxed) {
        return Err(BenchmarkError::Failed(
            "a benchmark worker stopped unexpectedly".into(),
        ));
    }
    analyze_sustained(windows)
        .ok_or_else(|| BenchmarkError::Failed("the sustained test produced no samples".into()))
}

/// Peak, median, sustained (last third) and decline over throughput windows.
pub fn analyze_sustained(windows: Vec<SustainedWindow>) -> Option<SustainedResult> {
    let mops: Vec<f64> = windows.iter().map(|w| w.mops).collect();
    let third = (mops.len() / 3).max(1);
    let first = stats::median(mops.get(..third)?)?;
    let last = stats::median(mops.get(mops.len() - third..)?)?;
    let peak = mops.iter().copied().fold(f64::MIN, f64::max);
    let decline = stats::percent_change(first, last).map_or(0.0, |c| (-c).max(0.0));

    let freq: Vec<f64> = windows.iter().filter_map(|w| w.frequency_mhz).collect();
    let freq_fell = freq.len() >= 3
        && stats::percent_change(
            stats::median(&freq[..third.min(freq.len())])?,
            stats::median(&freq[freq.len() - third.min(freq.len())..])?,
        )
        .is_some_and(|c| c < -10.0);

    // Describe what was measured; do not claim a cause that was not observed.
    let note = (decline >= 5.0).then(|| {
        let mut n = format!(
            "Performance decreased by {decline:.0}% during sustained load. \
             Temperature or power limits may have contributed."
        );
        if freq_fell {
            n.push_str(" CPU frequency also decreased over the same period.");
        }
        n
    });
    Some(SustainedResult {
        median_mops: stats::median(&mops)?,
        peak_mops: peak,
        sustained_mops: last,
        decline_percent: decline,
        windows,
        note,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn profile_worker_count_is_bounded_even_with_extreme_topology() {
        assert_eq!(
            CpuRunConfig::for_profile(BenchmarkProfile::Quick, 0).workers,
            1
        );
        assert_eq!(
            CpuRunConfig::for_profile(BenchmarkProfile::Extended, usize::MAX).workers,
            256
        );
    }

    struct NoHooks;
    impl CpuHooks for NoHooks {
        fn pin_current_thread(&self, _: usize) -> bool {
            false
        }
        fn frequency_mhz(&self) -> Option<f64> {
            None
        }
        fn temperature_c(&self) -> Option<f32> {
            None
        }
    }

    #[derive(Default)]
    struct Recorder {
        stages: Mutex<Vec<(BenchmarkState, String, f32)>>,
        lives: AtomicUsize,
    }
    impl Observer for Recorder {
        fn stage(&self, state: BenchmarkState, label: &str, fraction: f32) {
            self.stages
                .lock()
                .unwrap()
                .push((state, label.to_owned(), fraction));
        }
        fn live(&self, reading: Option<LiveReading>) {
            if reading.is_some() {
                self.lives.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    #[test]
    fn runs_every_workload_in_both_modes() {
        let cfg = CpuRunConfig::test();
        let rec = Recorder::default();
        let out = run_cpu(&cfg, &NoHooks, &CancelToken::new(), &rec).expect("runs");

        assert_eq!(out.workloads.len(), 10);
        for r in &out.workloads {
            assert_eq!(r.samples.len(), cfg.iterations, "{}", r.label);
            assert!(r.median > 0.0 && r.median.is_finite(), "{}", r.label);
            assert!(r.min <= r.median && r.median <= r.max);
            let expected = match r.mode {
                CpuMode::SingleCore => 1,
                CpuMode::MultiCore => cfg.workers as u32,
            };
            assert_eq!(r.workers, expected, "{}", r.label);
        }
        assert!(out.sustained.is_none());
        assert_eq!(out.affinity, "not pinned");

        let stages = rec.stages.lock().unwrap();
        let fractions: Vec<f32> = stages.iter().map(|s| s.2).collect();
        assert!(fractions.windows(2).all(|p| p[0] <= p[1]), "monotonic");
        assert!(fractions.iter().all(|f| (0.0..=1.0).contains(f)));
        assert!(*fractions.last().unwrap() > 0.9);
        assert!(rec.lives.load(Ordering::Relaxed) >= 10);
    }

    #[test]
    fn warmup_is_reported_but_not_measured() {
        let mut cfg = CpuRunConfig::test();
        cfg.warmup_iterations = 1;
        let rec = Recorder::default();
        let out = run_cpu(&cfg, &NoHooks, &CancelToken::new(), &rec).expect("runs");
        assert!(
            out.workloads
                .iter()
                .all(|r| r.samples.len() == cfg.iterations)
        );
        let stages = rec.stages.lock().unwrap();
        assert!(stages.iter().any(|s| s.0 == BenchmarkState::WarmingUp));
        assert!(stages.iter().any(|s| s.0 == BenchmarkState::Running));
    }

    #[test]
    fn single_core_uses_exactly_one_worker_and_reports_pinning() {
        let mut cfg = CpuRunConfig::test();
        cfg.pin_single = true;
        let out = run_cpu(&cfg, &NoHooks, &CancelToken::new(), &Recorder::default()).unwrap();
        // Pinning refused by the hooks: recorded, run still succeeds.
        assert!(out.affinity.contains("refused"));
        assert!(
            out.workloads
                .iter()
                .filter(|r| r.mode == CpuMode::SingleCore)
                .all(|r| r.workers == 1)
        );
    }

    #[test]
    fn cancellation_stops_the_run_without_a_result() {
        let cancel = CancelToken::new();
        cancel.cancel();
        let r = run_cpu(
            &CpuRunConfig::test(),
            &NoHooks,
            &cancel,
            &Recorder::default(),
        );
        assert!(matches!(r, Err(BenchmarkError::Cancelled)));
    }

    #[test]
    fn cancellation_during_a_long_run_returns_quickly() {
        // Far more work than could finish: only cancellation ends it.
        let mut cfg = CpuRunConfig::test();
        cfg.int_chunk_steps = u64::MAX / 64;
        let cancel = CancelToken::new();
        let c2 = cancel.clone();
        let canceller = thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            c2.cancel();
        });
        let started = Instant::now();
        let r = run_cpu(&cfg, &NoHooks, &cancel, &Recorder::default());
        canceller.join().unwrap();
        assert!(matches!(r, Err(BenchmarkError::Cancelled)));
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_panicking_worker_fails_the_run_instead_of_hanging() {
        let cfg = CpuRunConfig::test();
        let r = measure(
            3,
            8,
            None,
            &NoHooks,
            &cfg,
            &CancelToken::new(),
            |_, _| {},
            |chunk, _| if chunk == 1 { panic!("boom") } else { Ok(1) },
        );
        assert!(matches!(r, Err(BenchmarkError::Failed(m)) if m.contains("unexpectedly")));
    }

    #[test]
    fn output_that_changes_between_iterations_is_rejected() {
        let cfg = CpuRunConfig::test();
        let n = AtomicU64::new(0);
        let r = measure(
            1,
            1,
            None,
            &NoHooks,
            &cfg,
            &CancelToken::new(),
            |_, _| {},
            |_, _| Ok(n.fetch_add(1, Ordering::Relaxed)),
        );
        assert!(matches!(r, Err(BenchmarkError::Failed(m)) if m.contains("changed")));
    }

    #[test]
    fn an_invalid_workload_result_fails_the_run() {
        let cfg = CpuRunConfig::test();
        let r = measure(
            2,
            4,
            None,
            &NoHooks,
            &cfg,
            &CancelToken::new(),
            |_, _| {},
            |_, _| Err(WorkloadError::Invalid("nan")),
        );
        assert!(matches!(r, Err(BenchmarkError::Failed(m)) if m.contains("nan")));
    }

    #[test]
    fn every_chunk_runs_exactly_once_per_round_whoever_takes_it() {
        let cfg = CpuRunConfig::test();
        let counts: Vec<AtomicUsize> = (0..40).map(|_| AtomicUsize::new(0)).collect();
        measure(
            4,
            40,
            None,
            &NoHooks,
            &cfg,
            &CancelToken::new(),
            |_, _| {},
            |chunk, _| {
                counts[chunk].fetch_add(1, Ordering::Relaxed);
                // Uneven speeds, like performance and efficiency cores.
                if chunk % 3 == 0 {
                    thread::sleep(Duration::from_micros(300));
                }
                Ok(chunk as u64 + 1)
            },
        )
        .expect("runs");
        let rounds = cfg.timed_runs();
        assert!(counts.iter().all(|c| c.load(Ordering::Relaxed) == rounds));
    }

    fn windows(mops: &[f64], freq: &[Option<f64>]) -> Vec<SustainedWindow> {
        mops.iter()
            .enumerate()
            .map(|(i, m)| SustainedWindow {
                t_secs: i as f64 + 1.0,
                mops: *m,
                frequency_mhz: freq.get(i).copied().flatten(),
                temperature_c: None,
            })
            .collect()
    }

    #[test]
    fn sustained_analysis_reports_decline_without_claiming_a_cause() {
        let steady = analyze_sustained(windows(&[100.0; 9], &[])).unwrap();
        assert_eq!(steady.decline_percent, 0.0);
        assert!(steady.note.is_none());

        let mut fall = vec![100.0; 3];
        fall.extend([90.0; 3]);
        fall.extend([70.0; 3]);
        let r = analyze_sustained(windows(&fall, &[])).unwrap();
        assert_eq!(r.peak_mops, 100.0);
        assert_eq!(r.sustained_mops, 70.0);
        assert!((r.decline_percent - 30.0).abs() < 1e-9);
        let note = r.note.unwrap();
        assert!(note.contains("may have contributed"));
        assert!(!note.to_lowercase().contains("throttl"));
        assert!(!note.contains("frequency"), "no frequency data was given");

        // Frequency falling with throughput is reported as an observation.
        let freqs: Vec<Option<f64>> = [
            4000.0, 4000.0, 4000.0, 3500.0, 3500.0, 3500.0, 3000.0, 3000.0, 3000.0,
        ]
        .iter()
        .map(|f| Some(*f))
        .collect();
        let with_freq = analyze_sustained(windows(&fall, &freqs)).unwrap();
        assert!(with_freq.note.unwrap().contains("frequency also decreased"));
    }

    #[test]
    fn sustained_analysis_handles_few_windows_and_none() {
        assert!(analyze_sustained(Vec::new()).is_none());
        let one = analyze_sustained(windows(&[50.0], &[])).unwrap();
        assert_eq!(
            (one.peak_mops, one.sustained_mops, one.decline_percent),
            (50.0, 50.0, 0.0)
        );
    }

    #[test]
    fn sustained_phase_runs_and_stops() {
        let mut cfg = CpuRunConfig::test();
        cfg.sustained = Some(Duration::from_millis(400));
        let out = run_cpu(&cfg, &NoHooks, &CancelToken::new(), &Recorder::default()).unwrap();
        let s = out.sustained.expect("sustained");
        assert!(!s.windows.is_empty());
        assert!(s.windows.iter().all(|w| w.mops > 0.0 && w.mops.is_finite()));
    }
}
