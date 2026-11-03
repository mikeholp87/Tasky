//! Runs one benchmark at a time and owns everything around it: the lock,
//! the state machine, rate-limited progress, cancellation, saving, and
//! cleanup on every exit path (success, failure, cancel, panic).

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use super::cancel::CancelToken;
use super::cpu::runner::{CpuHooks, CpuRunConfig, Observer, run_cpu};
use super::gpu::runner::{GpuRunConfig, run_gpu};
use super::gpu::wgpu_backend::WgpuBackend;
use super::gpu::{self, GpuBackend};
use super::history::HistoryStore;
use super::model::{
    BenchmarkKind, BenchmarkProfile, BenchmarkResult, BenchmarkState, BenchmarkStatus,
    GpuAdapterInfo, GpuDeviceType, GpuResult, LiveReading,
};
use super::{BenchmarkError, environment, scoring};
use crate::errors::{AppError, AppResult};
use crate::models::TemperatureReading;
use crate::monitoring::{Engine, unix_millis};
use crate::platform;

/// The UI never needs more than this many progress updates per second.
const MIN_EMIT_INTERVAL: Duration = Duration::from_millis(100);

/// Receives progress. Called from the benchmark thread; must not block.
pub trait ProgressSink: Send + Sync {
    fn status(&self, status: &BenchmarkStatus);
    fn finished(&self, result: &BenchmarkResult);
}

struct ActiveRun {
    cancel: CancelToken,
}

struct Shared {
    status: Mutex<BenchmarkStatus>,
    /// `Some` while a run owns the benchmark lock.
    active: Mutex<Option<ActiveRun>>,
    counter: AtomicU64,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

pub struct Coordinator {
    engine: Arc<Engine>,
    history: Arc<HistoryStore>,
    shared: Arc<Shared>,
    gpu: Arc<dyn GpuBackend>,
    /// Tests run unoptimised; this lets them exercise scoring and saving.
    score_development_builds: bool,
}

/// What a run executes. The user chooses only the kind, profile and adapter;
/// explicit configurations exist for tests.
pub enum RunPlan {
    Cpu(CpuRunConfig),
    Gpu {
        /// `None` means "build it from the profile once the adapter is known".
        config: Option<GpuRunConfig>,
        adapter: Option<String>,
    },
}

impl From<CpuRunConfig> for RunPlan {
    fn from(cfg: CpuRunConfig) -> Self {
        Self::Cpu(cfg)
    }
}

impl Coordinator {
    pub fn new(engine: Arc<Engine>, data_dir: Option<&Path>) -> Self {
        Self {
            engine,
            history: Arc::new(HistoryStore::load(data_dir)),
            shared: Arc::new(Shared {
                status: Mutex::new(BenchmarkStatus::idle()),
                active: Mutex::new(None),
                counter: AtomicU64::new(0),
            }),
            gpu: Arc::new(WgpuBackend::new()),
            score_development_builds: false,
        }
    }

    /// Replaces the GPU backend (tests use a fake, so CI needs no GPU).
    #[doc(hidden)]
    pub fn with_gpu_backend(mut self, backend: Arc<dyn GpuBackend>) -> Self {
        self.gpu = backend;
        self
    }

    /// Graphics adapters a GPU benchmark could run on.
    pub fn gpu_adapters(&self) -> AppResult<Vec<GpuAdapterInfo>> {
        self.gpu.adapters().map_err(AppError::from)
    }

    #[doc(hidden)]
    pub fn score_development_builds(mut self, yes: bool) -> Self {
        self.score_development_builds = yes;
        self
    }

    pub fn status(&self) -> BenchmarkStatus {
        lock(&self.shared.status).clone()
    }

    pub fn history(&self) -> Vec<BenchmarkResult> {
        self.history.list()
    }

    pub fn delete_result(&self, id: &str) -> AppResult<()> {
        self.history.delete(id)
    }

    /// Asks the running benchmark to stop. No-op when idle.
    pub fn cancel(&self) {
        if let Some(run) = lock(&self.shared.active).as_ref() {
            run.cancel.cancel();
        }
    }

    /// `adapter` selects a graphics adapter for GPU runs (an id from
    /// [`gpu_adapters`](Self::gpu_adapters)); it is ignored for CPU runs.
    pub fn start(
        &self,
        kind: BenchmarkKind,
        profile: BenchmarkProfile,
        adapter: Option<String>,
        sink: Arc<dyn ProgressSink>,
    ) -> AppResult<BenchmarkStatus> {
        let plan = match kind {
            BenchmarkKind::Cpu => {
                let logical = self.engine.static_info().logical_core_count as usize;
                RunPlan::Cpu(CpuRunConfig::for_profile(profile, logical))
            }
            BenchmarkKind::Gpu => RunPlan::Gpu {
                config: None,
                adapter,
            },
            // Wired up when the disk and internet runs are connected below.
            BenchmarkKind::Disk | BenchmarkKind::Internet => {
                return Err(AppError::InvalidArgument("Not available yet.".into()));
            }
        };
        self.start_with(kind, profile, plan, sink)
    }

    /// [`start`](Self::start) with an explicit plan (tests use tiny ones).
    pub fn start_with(
        &self,
        kind: BenchmarkKind,
        profile: BenchmarkProfile,
        plan: impl Into<RunPlan>,
        sink: Arc<dyn ProgressSink>,
    ) -> AppResult<BenchmarkStatus> {
        let plan = plan.into();
        let cancel = CancelToken::new();
        let prefix = match kind {
            BenchmarkKind::Cpu => "cpu",
            BenchmarkKind::Gpu => "gpu",
            BenchmarkKind::Disk => "disk",
            BenchmarkKind::Internet => "net",
        };
        let id = format!(
            "{prefix}-{}-{}",
            unix_millis(),
            self.shared.counter.fetch_add(1, Ordering::Relaxed)
        );
        let initial = BenchmarkStatus {
            state: BenchmarkState::Preparing,
            run_id: Some(id.clone()),
            kind: Some(kind),
            profile: Some(profile),
            stage: "Starting".into(),
            ..BenchmarkStatus::idle()
        };
        {
            // Claim the lock and publish the first status together.
            let mut active = lock(&self.shared.active);
            if active.is_some() {
                return Err(AppError::BenchmarkAlreadyRunning);
            }
            *active = Some(ActiveRun {
                cancel: cancel.clone(),
            });
            *lock(&self.shared.status) = initial.clone();
        }
        sink.status(&initial);
        self.engine.record_activity(
            crate::monitoring::activity::Pending::new(
                crate::models::ActivityKind::BenchmarkStarted,
            )
            .detail(format!("{kind:?} benchmark, {profile:?} profile")),
        );

        let ctx = RunContext {
            engine: Arc::clone(&self.engine),
            history: Arc::clone(&self.history),
            shared: Arc::clone(&self.shared),
            gpu: Arc::clone(&self.gpu),
            sink,
            score_development_builds: self.score_development_builds,
            id,
            kind,
            profile,
            plan,
            cancel,
        };
        let shared = Arc::clone(&self.shared);
        std::thread::Builder::new()
            .name("benchmark".into())
            .spawn(move || ctx.run())
            .map_err(|e| {
                *lock(&shared.active) = None;
                *lock(&shared.status) = BenchmarkStatus::idle();
                AppError::BenchmarkFailed(format!("could not start the worker thread: {e}"))
            })?;
        Ok(initial)
    }
}

struct RunContext {
    engine: Arc<Engine>,
    history: Arc<HistoryStore>,
    shared: Arc<Shared>,
    gpu: Arc<dyn GpuBackend>,
    sink: Arc<dyn ProgressSink>,
    score_development_builds: bool,
    id: String,
    kind: BenchmarkKind,
    profile: BenchmarkProfile,
    plan: RunPlan,
    cancel: CancelToken,
}

/// Clears the benchmark lock on any exit, including a panic that escapes
/// the normal finalisation.
struct LockGuard(Option<Arc<Shared>>);
impl Drop for LockGuard {
    fn drop(&mut self) {
        if let Some(shared) = &self.0 {
            *lock(&shared.active) = None;
        }
    }
}

impl RunContext {
    fn run(self) {
        let mut guard = LockGuard(Some(Arc::clone(&self.shared)));
        let reporter = Reporter::new(&self);
        let outcome = catch_unwind(AssertUnwindSafe(|| self.execute(&reporter)));
        let (state, message, result) = match outcome {
            Ok(Ok(result)) => {
                let saved = if result.environment.development_build {
                    Ok(())
                } else {
                    self.history.add(result.clone())
                };
                let message = saved
                    .err()
                    .map(|e| format!("The result could not be saved: {e}"));
                (BenchmarkState::Completed, message, Some(result))
            }
            Ok(Err(BenchmarkError::Cancelled)) => (
                BenchmarkState::Cancelled,
                Some("Cancelled. Nothing was saved.".to_owned()),
                None,
            ),
            Ok(Err(e)) => (BenchmarkState::Failed, Some(e.to_string()), None),
            Err(_) => (
                BenchmarkState::Failed,
                Some("The benchmark stopped unexpectedly.".to_owned()),
                None,
            ),
        };
        tracing::info!(id = %self.id, ?state, "benchmark finished");

        // Publish the final status and release the lock as one step, so a
        // new run can never be overwritten by the old run's last update.
        let final_status = {
            let mut active = lock(&self.shared.active);
            let mut status = lock(&self.shared.status);
            status.state = state;
            status.message = message;
            if state == BenchmarkState::Completed {
                status.fraction = 1.0;
                status.stage = "Done".into();
            }
            status.live = None;
            let snapshot = status.clone();
            *active = None;
            // A subsequent run may start as soon as this mutex is released.
            // The old run's panic guard must never clear the new run's lock.
            guard.0 = None;
            snapshot
        };
        if let Some(result) = &result {
            self.sink.finished(result);
        }
        self.sink.status(&final_status);
        // Also recorded when cancelled or failed; the detail says which.
        self.engine.record_activity(
            crate::monitoring::activity::Pending::new(
                crate::models::ActivityKind::BenchmarkCompleted,
            )
            .detail(format!("{:?} benchmark: {state:?}", self.kind)),
        );
    }

    fn execute(&self, reporter: &Reporter) -> Result<BenchmarkResult, BenchmarkError> {
        let started_ms = unix_millis();
        let started = Instant::now();
        reporter.set_state(
            BenchmarkState::Preflight,
            match self.plan {
                RunPlan::Cpu(_) => "Checking the system",
                RunPlan::Gpu { .. } => "Checking graphics adapters",
            },
        );

        let background = self.engine.latest().map(|t| t.cpu.usage_percent);
        let mut env = environment::collect(self.engine.static_info(), background);
        if self.score_development_builds {
            env.development_build = false;
        }
        let mut warnings = environment::warnings(&env);
        if self.cancel.is_cancelled() {
            return Err(BenchmarkError::Cancelled);
        }

        let mut result = BenchmarkResult {
            id: self.id.clone(),
            kind: self.kind,
            profile: self.profile,
            started_at_ms: started_ms,
            duration_ms: 0,
            method: scoring::cpu_method(),
            environment: env.clone(),
            workloads: Vec::new(),
            sustained: None,
            scores: None,
            gpu: None,
            gpu_score: None,
            disk: None,
            disk_score: None,
            internet: None,
            warnings: Vec::new(),
            stability: super::model::Stability::High,
            outlier_method: "1.5×IQR; flagged, none removed".into(),
            affinity: "not pinned".into(),
        };

        match &self.plan {
            RunPlan::Cpu(cfg) => {
                let hooks = EngineHooks(&self.engine);
                let out = run_cpu(cfg, &hooks, &self.cancel, reporter)?;
                reporter.set_state(BenchmarkState::Finalizing, "Calculating results");
                let scores = if env.development_build {
                    None
                } else {
                    scoring::cpu_scores(&out.workloads)
                };
                if scores.is_none() && !env.development_build {
                    warnings.push(
                        "Some measurements were unavailable, so no score was calculated.".into(),
                    );
                }
                result.stability = scoring::stability(&out.workloads, &warnings);
                result.workloads = out.workloads;
                result.sustained = out.sustained;
                result.scores = scores;
                result.affinity = out.affinity;
            }
            RunPlan::Gpu { config, adapter } => {
                let profile = self.profile;
                let (info, mut exec, cfg) = self.gpu.open(adapter.as_deref(), &|a| {
                    config.clone().unwrap_or_else(|| {
                        GpuRunConfig::for_profile(profile, a.device_type == GpuDeviceType::Discrete)
                    })
                })?;
                let out = run_gpu(&mut *exec, &cfg, &self.cancel, reporter)?;
                // Release the device before the (possibly slow) bookkeeping.
                drop(exec);
                reporter.set_state(BenchmarkState::Finalizing, "Calculating results");
                let software = info.device_type == GpuDeviceType::Software;
                let score = if env.development_build || software {
                    None
                } else {
                    scoring::gpu_score(&out.workloads)
                };
                if score.is_none() && !env.development_build && !software {
                    warnings.push(
                        "Some measurements were unavailable, so no score was calculated.".into(),
                    );
                }
                result.method = scoring::gpu_method();
                result.stability = scoring::gpu_stability(&out.workloads, &warnings);
                result.gpu_score = score;
                result.sustained = None;
                result.gpu = Some(GpuResult {
                    notes: gpu::notes(&info, out.timing_method),
                    adapter: info,
                    timing_method: out.timing_method,
                    workloads: out.workloads,
                    sustained: out.sustained,
                });
            }
        }
        result.warnings = warnings;
        result.duration_ms = started.elapsed().as_millis() as u64;
        Ok(result)
    }
}

/// Frequency and temperature come from the monitoring engine's latest tick,
/// so the benchmark adds no sensor reads of its own.
struct EngineHooks<'a>(&'a Engine);

impl CpuHooks for EngineHooks<'_> {
    fn pin_current_thread(&self, slot: usize) -> bool {
        platform::pin_current_thread(slot)
    }

    fn frequency_mhz(&self) -> Option<f64> {
        self.0.latest()?.cpu.frequency_mhz.map(|f| f as f64)
    }

    fn temperature_c(&self) -> Option<f32> {
        cpu_temperature(&self.0.latest()?.temperatures)
    }
}

/// The CPU package temperature, only when a sensor label identifies it. A
/// generic `temp1` is never assumed to be the CPU.
pub fn cpu_temperature(readings: &[TemperatureReading]) -> Option<f32> {
    const PREFERRED: [&str; 6] = [
        "coretemp Package",
        "k10temp Tctl",
        "k10temp Tdie",
        "zenpower Tdie",
        "x86_pkg_temp",
        "cpu_thermal",
    ];
    PREFERRED.iter().find_map(|prefix| {
        readings
            .iter()
            .find(|r| r.label.starts_with(prefix))
            .map(|r| r.celsius)
    })
}

/// Turns the runner's callbacks into rate-limited status updates.
struct Reporter {
    shared: Arc<Shared>,
    sink: Arc<dyn ProgressSink>,
    engine: Arc<Engine>,
    started: Instant,
    last_emit: Mutex<Option<Instant>>,
}

impl Reporter {
    fn new(ctx: &RunContext) -> Self {
        Self {
            shared: Arc::clone(&ctx.shared),
            sink: Arc::clone(&ctx.sink),
            engine: Arc::clone(&ctx.engine),
            started: Instant::now(),
            last_emit: Mutex::new(None),
        }
    }

    fn set_state(&self, state: BenchmarkState, stage: &str) {
        self.update(|s| {
            s.state = state;
            s.stage = stage.to_owned();
        });
    }

    /// Applies `change`, then emits unless an update went out less than
    /// [`MIN_EMIT_INTERVAL`] ago and the state did not change.
    fn update(&self, change: impl FnOnce(&mut BenchmarkStatus)) {
        let tick = self.engine.latest();
        let (snapshot, state_changed) = {
            let mut status = lock(&self.shared.status);
            // A finished or cancelled run's late callbacks must not revive it.
            if !status.state.is_active() {
                return;
            }
            let before = status.state;
            change(&mut status);
            status.elapsed_ms = self.started.elapsed().as_millis() as u64;
            if let Some(t) = tick {
                status.cpu_percent = Some(t.cpu.usage_percent);
                status.frequency_mhz = t.cpu.frequency_mhz.map(|f| f as f64);
                status.temperature_c = cpu_temperature(&t.temperatures);
            }
            (status.clone(), status.state != before)
        };
        let mut last = lock(&self.last_emit);
        if state_changed || last.is_none_or(|t| t.elapsed() >= MIN_EMIT_INTERVAL) {
            *last = Some(Instant::now());
            drop(last);
            self.sink.status(&snapshot);
        }
    }
}

impl Observer for Reporter {
    fn stage(&self, state: BenchmarkState, label: &str, fraction: f32) {
        self.update(|s| {
            s.state = state;
            s.stage = label.to_owned();
            s.fraction = fraction.clamp(0.0, 1.0);
        });
    }

    fn live(&self, reading: Option<LiveReading>) {
        self.update(|s| s.live = reading);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmarks::model::{CpuScores, PowerSource};
    use crate::models::MonitoringState;

    #[derive(Default)]
    struct Recorder {
        statuses: Mutex<Vec<BenchmarkStatus>>,
        results: Mutex<Vec<BenchmarkResult>>,
    }
    impl ProgressSink for Recorder {
        fn status(&self, s: &BenchmarkStatus) {
            lock(&self.statuses).push(s.clone());
        }
        fn finished(&self, r: &BenchmarkResult) {
            lock(&self.results).push(r.clone());
        }
    }

    fn coordinator() -> Coordinator {
        let engine = Arc::new(Engine::start(MonitoringState::default()).expect("engine"));
        Coordinator::new(engine, None).score_development_builds(true)
    }

    fn wait_until_idle(c: &Coordinator) -> BenchmarkStatus {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let s = c.status();
            if !s.state.is_active() && lock(&c.shared.active).is_none() {
                return s;
            }
            assert!(Instant::now() < deadline, "benchmark did not finish");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn quick(
        c: &Coordinator,
        sink: &Arc<Recorder>,
        cfg: CpuRunConfig,
    ) -> AppResult<BenchmarkStatus> {
        let sink: Arc<dyn ProgressSink> = sink.clone();
        c.start_with(BenchmarkKind::Cpu, BenchmarkProfile::Quick, cfg, sink)
    }

    #[test]
    fn a_finished_runs_guard_cannot_release_the_next_runs_lock() {
        use std::sync::{Barrier, mpsc};
        struct Finishing {
            entered: mpsc::Sender<()>,
            release: Arc<Barrier>,
            dropped: mpsc::Sender<()>,
        }
        impl ProgressSink for Finishing {
            fn status(&self, _: &BenchmarkStatus) {}
            fn finished(&self, _: &BenchmarkResult) {
                self.entered.send(()).unwrap();
                self.release.wait();
            }
        }
        impl Drop for Finishing {
            fn drop(&mut self) {
                let _ = self.dropped.send(());
            }
        }
        let c = coordinator();
        let (entered, ready) = mpsc::channel();
        let (dropped, done) = mpsc::channel();
        let release = Arc::new(Barrier::new(2));
        c.start_with(
            BenchmarkKind::Cpu,
            BenchmarkProfile::Quick,
            CpuRunConfig::test(),
            Arc::new(Finishing {
                entered,
                release: Arc::clone(&release),
                dropped,
            }),
        )
        .unwrap();
        ready.recv_timeout(Duration::from_secs(30)).unwrap();
        let mut long = CpuRunConfig::test();
        long.int_chunk_steps = u64::MAX / 4096;
        let sink = Arc::new(Recorder::default());
        quick(&c, &sink, long).unwrap();
        release.wait();
        done.recv_timeout(Duration::from_secs(10)).unwrap();
        let third = quick(&c, &sink, CpuRunConfig::test());
        c.cancel();
        wait_until_idle(&c);
        assert!(matches!(third, Err(AppError::BenchmarkAlreadyRunning)));
    }

    #[test]
    fn a_full_run_completes_scores_and_saves() {
        let c = coordinator();
        let sink = Arc::new(Recorder::default());
        let started = quick(&c, &sink, CpuRunConfig::test()).expect("starts");
        assert_eq!(started.state, BenchmarkState::Preparing);

        let done = wait_until_idle(&c);
        assert_eq!(done.state, BenchmarkState::Completed);
        assert_eq!(done.fraction, 1.0);

        let results = lock(&sink.results);
        assert_eq!(results.len(), 1);
        let r = &results[0];
        assert_eq!(r.workloads.len(), 10);
        assert!(
            matches!(r.scores, Some(CpuScores { single_core, multi_core }) if single_core > 0 && multi_core > 0)
        );
        assert_eq!(r.method.method_id, "cpu-method-v1");
        assert!(!r.environment.development_build);
        assert_eq!(c.history().len(), 1);
        assert_eq!(c.history()[0].id, r.id);

        // The state machine only moves forward through the expected states.
        let states: Vec<BenchmarkState> = lock(&sink.statuses).iter().map(|s| s.state).collect();
        let order = |s: &BenchmarkState| match s {
            BenchmarkState::Preparing => 0,
            BenchmarkState::Preflight => 1,
            BenchmarkState::WarmingUp | BenchmarkState::Running => 2,
            BenchmarkState::Finalizing => 3,
            BenchmarkState::Completed => 4,
            other => panic!("unexpected state {other:?}"),
        };
        assert!(
            states.windows(2).all(|p| order(&p[0]) <= order(&p[1])),
            "{states:?}"
        );
        assert_eq!(states.last(), Some(&BenchmarkState::Completed));
    }

    #[test]
    fn only_one_benchmark_runs_at_a_time() {
        let c = coordinator();
        let sink = Arc::new(Recorder::default());
        let mut long = CpuRunConfig::test();
        long.int_chunk_steps = u64::MAX / 4096;
        quick(&c, &sink, long).expect("first starts");
        let second = quick(&c, &sink, CpuRunConfig::test());
        assert!(matches!(second, Err(AppError::BenchmarkAlreadyRunning)));
        c.cancel();
        wait_until_idle(&c);
        // The lock is free again.
        quick(&c, &sink, CpuRunConfig::test()).expect("can start again");
        assert_eq!(wait_until_idle(&c).state, BenchmarkState::Completed);
    }

    #[test]
    fn cancelling_stops_the_run_saves_nothing_and_frees_the_lock() {
        let c = coordinator();
        let sink = Arc::new(Recorder::default());
        let mut long = CpuRunConfig::test();
        long.int_chunk_steps = u64::MAX / 4096;
        quick(&c, &sink, long).expect("starts");
        std::thread::sleep(Duration::from_millis(150));
        let t = Instant::now();
        c.cancel();
        let done = wait_until_idle(&c);
        assert!(t.elapsed() < Duration::from_secs(3), "{:?}", t.elapsed());
        assert_eq!(done.state, BenchmarkState::Cancelled);
        assert!(
            done.message
                .is_some_and(|m| m.contains("Nothing was saved"))
        );
        assert!(c.history().is_empty());
        assert!(lock(&sink.results).is_empty());
    }

    #[test]
    fn cancelling_while_idle_does_nothing() {
        let c = coordinator();
        c.cancel();
        assert_eq!(c.status().state, BenchmarkState::Idle);
    }

    #[test]
    fn development_builds_are_flagged_unscored_and_not_saved() {
        let engine = Arc::new(Engine::start(MonitoringState::default()).expect("engine"));
        let c = Coordinator::new(engine, None); // default: treat debug as development
        let sink = Arc::new(Recorder::default());
        quick(&c, &sink, CpuRunConfig::test()).expect("starts");
        let done = wait_until_idle(&c);
        assert_eq!(done.state, BenchmarkState::Completed);
        if cfg!(debug_assertions) {
            let results = lock(&sink.results);
            let r = &results[0];
            assert!(r.environment.development_build);
            assert!(r.scores.is_none());
            assert!(r.warnings.iter().any(|w| w.contains("Development build")));
            assert!(c.history().is_empty());
        }
    }

    #[test]
    fn progress_updates_are_rate_limited() {
        let c = coordinator();
        let sink = Arc::new(Recorder::default());
        let dyn_sink: Arc<dyn ProgressSink> = sink.clone();
        {
            let ctx_status = BenchmarkStatus {
                state: BenchmarkState::Running,
                ..BenchmarkStatus::idle()
            };
            *lock(&c.shared.status) = ctx_status;
        }
        let reporter = Reporter {
            shared: Arc::clone(&c.shared),
            sink: dyn_sink,
            engine: Arc::clone(&c.engine),
            started: Instant::now(),
            last_emit: Mutex::new(None),
        };
        let t = Instant::now();
        for i in 0..5000 {
            reporter.live(Some(LiveReading {
                label: "x".into(),
                value: f64::from(i),
                unit: "u".into(),
            }));
        }
        let emitted = lock(&sink.statuses).len();
        let max = 1 + (t.elapsed().as_millis() / MIN_EMIT_INTERVAL.as_millis()) as usize;
        assert!(
            emitted >= 1 && emitted <= max,
            "{emitted} updates in {:?}",
            t.elapsed()
        );
        // The latest value is always the one stored, even when not emitted.
        assert_eq!(
            lock(&c.shared.status).live.as_ref().map(|l| l.value),
            Some(4999.0)
        );
        // A state change always goes out immediately.
        reporter.stage(BenchmarkState::Finalizing, "Done soon", 0.9);
        assert_eq!(
            lock(&sink.statuses).last().map(|s| s.state),
            Some(BenchmarkState::Finalizing)
        );
    }

    #[test]
    fn late_callbacks_do_not_revive_a_finished_run() {
        let c = coordinator();
        let sink = Arc::new(Recorder::default());
        let dyn_sink: Arc<dyn ProgressSink> = sink.clone();
        *lock(&c.shared.status) = BenchmarkStatus {
            state: BenchmarkState::Cancelled,
            ..BenchmarkStatus::idle()
        };
        let reporter = Reporter {
            shared: Arc::clone(&c.shared),
            sink: dyn_sink,
            engine: Arc::clone(&c.engine),
            started: Instant::now(),
            last_emit: Mutex::new(None),
        };
        reporter.stage(BenchmarkState::Running, "late", 0.5);
        assert_eq!(c.status().state, BenchmarkState::Cancelled);
        assert!(lock(&sink.statuses).is_empty());
    }

    fn reading(label: &str, c: f32) -> TemperatureReading {
        TemperatureReading {
            label: label.into(),
            celsius: c,
            max_celsius: None,
            critical_celsius: None,
        }
    }

    #[test]
    fn cpu_temperature_needs_an_identifying_label() {
        // `temp1` is never assumed to be the CPU.
        assert_eq!(
            cpu_temperature(&[
                reading("acpitz temp1", 27.0),
                reading("nvme Composite", 40.0)
            ]),
            None
        );
        assert_eq!(
            cpu_temperature(&[
                reading("coretemp Core 0", 60.0),
                reading("acpitz temp1", 27.0),
                reading("coretemp Package id 0", 62.0),
            ]),
            Some(62.0)
        );
        assert_eq!(
            cpu_temperature(&[reading("k10temp Tctl", 55.0)]),
            Some(55.0)
        );
        assert_eq!(cpu_temperature(&[]), None);
        // Environment type is available to tests that need it.
        let _ = PowerSource::Unknown;
    }

    // ------------------------------------------------------------- GPU
    use crate::benchmarks::gpu::adapters;
    use crate::benchmarks::gpu::runner::GpuExecutor;
    use crate::benchmarks::gpu::runner::tests::Fake;

    struct FakeBackend {
        adapters: Vec<GpuAdapterInfo>,
        fail: Option<String>,
    }

    impl GpuBackend for FakeBackend {
        fn adapters(&self) -> Result<Vec<GpuAdapterInfo>, BenchmarkError> {
            Ok(self.adapters.clone())
        }

        fn open(
            &self,
            id: Option<&str>,
            config_for: &dyn Fn(&GpuAdapterInfo) -> GpuRunConfig,
        ) -> Result<(GpuAdapterInfo, Box<dyn GpuExecutor>, GpuRunConfig), BenchmarkError> {
            if let Some(m) = &self.fail {
                return Err(BenchmarkError::Failed(m.clone()));
            }
            let info = adapters::select(&self.adapters, id)?.clone();
            let cfg = config_for(&info);
            Ok((info, Box::new(Fake::default()), cfg))
        }
    }

    fn adapter(id: &str, t: GpuDeviceType) -> GpuAdapterInfo {
        GpuAdapterInfo {
            id: id.into(),
            name: format!("Test {id}"),
            vendor_id: 1,
            device_id: 2,
            device_type: t,
            backend: "vulkan".into(),
            driver: "test".into(),
            driver_info: "1.0".into(),
            supports_timestamps: true,
            max_buffer_bytes: 1 << 30,
            max_storage_binding_bytes: 1 << 27,
            max_workgroups_per_dimension: 65_535,
            recommended: false,
        }
    }

    fn gpu_coordinator(list: Vec<GpuAdapterInfo>, fail: Option<&str>) -> Coordinator {
        coordinator().with_gpu_backend(Arc::new(FakeBackend {
            adapters: list,
            fail: fail.map(str::to_owned),
        }))
    }

    fn start_gpu(
        c: &Coordinator,
        sink: &Arc<Recorder>,
        adapter: Option<&str>,
    ) -> AppResult<BenchmarkStatus> {
        let sink: Arc<dyn ProgressSink> = sink.clone();
        c.start_with(
            BenchmarkKind::Gpu,
            BenchmarkProfile::Quick,
            RunPlan::Gpu {
                config: Some(GpuRunConfig::test()),
                adapter: adapter.map(str::to_owned),
            },
            sink,
        )
    }

    #[test]
    fn a_gpu_run_completes_scores_saves_and_records_the_adapter() {
        let c = gpu_coordinator(
            vec![
                adapter("cpu-like", GpuDeviceType::Software),
                adapter("igpu", GpuDeviceType::Integrated),
            ],
            None,
        );
        let sink = Arc::new(Recorder::default());
        start_gpu(&c, &sink, None).expect("starts");
        let done = wait_until_idle(&c);
        assert_eq!(done.state, BenchmarkState::Completed, "{:?}", done.message);

        let r = lock(&sink.results)[0].clone();
        assert_eq!(r.kind, BenchmarkKind::Gpu);
        assert_eq!(r.method.method_id, "gpu-compute-v1");
        let g = r.gpu.as_ref().expect("gpu result");
        // The software renderer is never the default.
        assert_eq!(g.adapter.id, "igpu");
        assert_eq!(g.workloads.len(), 3);
        assert!(g.workloads.iter().all(|w| w.validated));
        assert!(r.gpu_score.is_some_and(|s| s > 0));
        assert!(r.scores.is_none() && r.workloads.is_empty());
        assert!(g.notes.iter().any(|n| n.contains("shared system memory")));
        assert!(r.id.starts_with("gpu-"));
        // Saved, and survives a JSON round trip unchanged.
        assert_eq!(c.history().len(), 1);
        let json = serde_json::to_string(&r).unwrap();
        assert_eq!(serde_json::from_str::<BenchmarkResult>(&json).unwrap(), r);
    }

    #[test]
    fn a_software_renderer_is_run_on_request_but_never_scored() {
        let c = gpu_coordinator(vec![adapter("llvmpipe", GpuDeviceType::Software)], None);
        let sink = Arc::new(Recorder::default());
        start_gpu(&c, &sink, Some("llvmpipe")).expect("starts");
        assert_eq!(wait_until_idle(&c).state, BenchmarkState::Completed);
        let r = lock(&sink.results)[0].clone();
        assert_eq!(r.gpu_score, None);
        assert!(
            r.gpu
                .unwrap()
                .notes
                .iter()
                .any(|n| n.contains("software renderer"))
        );
        // Not scored, but also not a failure: no misleading "unavailable" warning.
        assert!(!r.warnings.iter().any(|w| w.contains("unavailable")));
    }

    #[test]
    fn no_adapter_fails_cleanly_and_frees_the_lock() {
        let c = gpu_coordinator(vec![], None);
        let sink = Arc::new(Recorder::default());
        start_gpu(&c, &sink, None).expect("starts");
        let done = wait_until_idle(&c);
        assert_eq!(done.state, BenchmarkState::Failed);
        assert!(
            done.message
                .is_some_and(|m| m.contains("No graphics adapter"))
        );
        assert!(c.history().is_empty());
        // The app keeps working: a CPU benchmark can run right after.
        quick(&c, &sink, CpuRunConfig::test()).expect("can start another");
        assert_eq!(wait_until_idle(&c).state, BenchmarkState::Completed);
    }

    #[test]
    fn a_driver_failure_is_reported_in_plain_words() {
        let c = gpu_coordinator(
            vec![adapter("igpu", GpuDeviceType::Integrated)],
            Some("The graphics adapter could not be opened: driver said no"),
        );
        let sink = Arc::new(Recorder::default());
        start_gpu(&c, &sink, None).expect("starts");
        let done = wait_until_idle(&c);
        assert_eq!(done.state, BenchmarkState::Failed);
        assert!(
            done.message
                .is_some_and(|m| m.contains("could not be opened"))
        );
    }

    #[test]
    fn a_choice_of_a_vanished_adapter_is_an_error_not_a_silent_fallback() {
        let c = gpu_coordinator(vec![adapter("igpu", GpuDeviceType::Integrated)], None);
        let sink = Arc::new(Recorder::default());
        start_gpu(&c, &sink, Some("gone")).expect("starts");
        let done = wait_until_idle(&c);
        assert_eq!(done.state, BenchmarkState::Failed);
        assert!(
            done.message
                .is_some_and(|m| m.contains("no longer available"))
        );
    }

    #[test]
    fn cpu_and_gpu_benchmarks_share_one_lock() {
        let c = gpu_coordinator(vec![adapter("igpu", GpuDeviceType::Integrated)], None);
        let sink = Arc::new(Recorder::default());
        let mut long = CpuRunConfig::test();
        long.int_chunk_steps = u64::MAX / 4096;
        quick(&c, &sink, long).expect("cpu starts");
        assert!(matches!(
            start_gpu(&c, &sink, None),
            Err(AppError::BenchmarkAlreadyRunning)
        ));
        c.cancel();
        wait_until_idle(&c);
        start_gpu(&c, &sink, None).expect("gpu can start after");
        assert_eq!(wait_until_idle(&c).state, BenchmarkState::Completed);
    }

    #[test]
    fn development_builds_do_not_score_gpu_runs_either() {
        let engine = Arc::new(Engine::start(MonitoringState::default()).expect("engine"));
        let c = Coordinator::new(engine, None).with_gpu_backend(Arc::new(FakeBackend {
            adapters: vec![adapter("igpu", GpuDeviceType::Integrated)],
            fail: None,
        }));
        let sink = Arc::new(Recorder::default());
        start_gpu(&c, &sink, None).expect("starts");
        assert_eq!(wait_until_idle(&c).state, BenchmarkState::Completed);
        if cfg!(debug_assertions) {
            let r = lock(&sink.results)[0].clone();
            assert!(r.environment.development_build && r.gpu_score.is_none());
            assert!(c.history().is_empty());
        }
    }

    #[test]
    fn the_adapter_list_comes_from_the_backend() {
        let c = gpu_coordinator(
            vec![
                adapter("a", GpuDeviceType::Discrete),
                adapter("b", GpuDeviceType::Integrated),
            ],
            None,
        );
        assert_eq!(c.gpu_adapters().unwrap().len(), 2);
    }
}
