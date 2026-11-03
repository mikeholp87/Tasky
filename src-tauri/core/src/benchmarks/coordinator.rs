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
use super::history::HistoryStore;
use super::model::{
    BenchmarkKind, BenchmarkProfile, BenchmarkResult, BenchmarkState, BenchmarkStatus, LiveReading,
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
    /// Tests run unoptimised; this lets them exercise scoring and saving.
    score_development_builds: bool,
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
            score_development_builds: false,
        }
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

    pub fn start(
        &self,
        kind: BenchmarkKind,
        profile: BenchmarkProfile,
        sink: Arc<dyn ProgressSink>,
    ) -> AppResult<BenchmarkStatus> {
        let logical = self.engine.static_info().logical_core_count as usize;
        let cfg = CpuRunConfig::for_profile(profile, logical);
        self.start_with(kind, profile, cfg, sink)
    }

    /// [`start`](Self::start) with an explicit workload (tests use tiny ones).
    pub fn start_with(
        &self,
        kind: BenchmarkKind,
        profile: BenchmarkProfile,
        cfg: CpuRunConfig,
        sink: Arc<dyn ProgressSink>,
    ) -> AppResult<BenchmarkStatus> {
        let cancel = CancelToken::new();
        let id = format!(
            "cpu-{}-{}",
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
            sink,
            score_development_builds: self.score_development_builds,
            id,
            kind,
            profile,
            cfg,
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
    sink: Arc<dyn ProgressSink>,
    score_development_builds: bool,
    id: String,
    kind: BenchmarkKind,
    profile: BenchmarkProfile,
    cfg: CpuRunConfig,
    cancel: CancelToken,
}

/// Clears the benchmark lock on any exit, including a panic that escapes
/// the normal finalisation.
struct LockGuard(Arc<Shared>);
impl Drop for LockGuard {
    fn drop(&mut self) {
        *lock(&self.0.active) = None;
    }
}

impl RunContext {
    fn run(self) {
        let _guard = LockGuard(Arc::clone(&self.shared));
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
        reporter.set_state(BenchmarkState::Preflight, "Checking the system");

        let background = self.engine.latest().map(|t| t.cpu.usage_percent);
        let mut env = environment::collect(self.engine.static_info(), background);
        if self.score_development_builds {
            env.development_build = false;
        }
        let mut warnings = environment::warnings(&env);
        if self.cancel.is_cancelled() {
            return Err(BenchmarkError::Cancelled);
        }

        let hooks = EngineHooks(&self.engine);
        let out = run_cpu(&self.cfg, &hooks, &self.cancel, reporter)?;

        reporter.set_state(BenchmarkState::Finalizing, "Calculating results");
        let scores = if env.development_build {
            None
        } else {
            scoring::cpu_scores(&out.workloads)
        };
        if scores.is_none() && !env.development_build {
            warnings.push("Some measurements were unavailable, so no score was calculated.".into());
        }
        Ok(BenchmarkResult {
            id: self.id.clone(),
            kind: self.kind,
            profile: self.profile,
            started_at_ms: started_ms,
            duration_ms: started.elapsed().as_millis() as u64,
            method: scoring::cpu_method(),
            environment: env,
            stability: scoring::stability(&out.workloads, &warnings),
            workloads: out.workloads,
            sustained: out.sustained,
            scores,
            warnings,
            outlier_method: "1.5×IQR; flagged, none removed".into(),
            affinity: out.affinity,
        })
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
}
