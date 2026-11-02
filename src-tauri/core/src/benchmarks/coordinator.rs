//! The single entry point for running benchmarks.
//!
//! ```text
//! start(request) ─► validate ─► claim run slot ─► "benchmark-runner" thread:
//!     Preparing (snapshot, Benchmark Mode on, interference monitor)
//!     for kind in CPU, GPU, Disk, Network:  [CoolingDown] → category.run()
//!     Finalizing (snapshot, scores, confidence, persist)
//!     Completed | Cancelled | Failed  → Finished event, slot released
//! ```
//!
//! Only one run exists at a time. Cancelled and failed runs never write to
//! history; completed runs are written atomically.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::cancellation::{CancelReason, CancelToken};
use super::environment::{
    self, BenchmarkModeGuard, InterferenceMonitor, SystemProbe, fingerprint, load_level,
    preflight_warnings,
};
use super::history::HistoryStore;
use super::models::{
    BenchmarkCapabilities, BenchmarkEvent, BenchmarkKind, BenchmarkPhase, BenchmarkRecord,
    BenchmarkRequest, BenchmarkWarning, CategoryResult, CategoryStatus, FailureInfo,
    PreflightReport, WarningCode,
};
use super::progress::{ProgressSink, Reporter};
use super::suite::{BenchmarkFactory, RunContext};
use super::{METHOD_VERSION, SCHEMA_VERSION, SCORE_VERSION, confidence, scoring, validate_request};
use crate::errors::{AppError, AppResult};
use crate::monitoring::unix_millis;

struct ActiveRun {
    id: String,
    cancel: Arc<CancelToken>,
    handle: Option<JoinHandle<()>>,
}

pub struct BenchmarkCoordinator {
    probe: Arc<dyn SystemProbe>,
    hardware: Arc<dyn BenchmarkFactory>,
    simulated: Arc<dyn BenchmarkFactory>,
    history: Arc<HistoryStore>,
    simulation_enabled: bool,
    save_history: AtomicBool,
    active: Mutex<Option<ActiveRun>>,
}

struct Outcome {
    phase: BenchmarkPhase,
    result_id: Option<String>,
    error: Option<FailureInfo>,
}

impl BenchmarkCoordinator {
    pub fn new(
        probe: Arc<dyn SystemProbe>,
        hardware: Arc<dyn BenchmarkFactory>,
        simulated: Arc<dyn BenchmarkFactory>,
        history: Arc<HistoryStore>,
        simulation_enabled: bool,
    ) -> Arc<Self> {
        Arc::new(Self {
            probe,
            hardware,
            simulated,
            history,
            simulation_enabled,
            save_history: AtomicBool::new(true),
            active: Mutex::new(None),
        })
    }

    pub fn history(&self) -> &HistoryStore {
        &self.history
    }

    pub fn set_save_history(&self, save: bool) {
        self.save_history.store(save, Ordering::SeqCst);
    }

    fn active(&self) -> MutexGuard<'_, Option<ActiveRun>> {
        self.active.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn running_run_id(&self) -> Option<String> {
        self.active()
            .as_ref()
            .filter(|r| r.handle.as_ref().is_some_and(|h| !h.is_finished()))
            .map(|r| r.id.clone())
    }

    pub fn capabilities(&self) -> BenchmarkCapabilities {
        let info = self.probe.static_info();
        BenchmarkCapabilities {
            method_version: METHOD_VERSION.into(),
            score_version: SCORE_VERSION,
            logical_cores: info.logical_core_count,
            physical_cores: info.physical_core_count,
            simulation_available: self.simulation_enabled,
            temperature_available: false,
            power_available: self.probe.power().source != super::models::PowerSource::Unknown,
            running_run_id: self.running_run_id(),
        }
    }

    fn factory(&self, request: &BenchmarkRequest) -> &dyn BenchmarkFactory {
        if request.simulate {
            &*self.simulated
        } else {
            &*self.hardware
        }
    }

    fn validate(&self, request: BenchmarkRequest) -> AppResult<BenchmarkRequest> {
        validate_request(
            request,
            self.probe.static_info().logical_core_count,
            self.simulation_enabled,
        )
    }

    pub fn preflight(&self, request: BenchmarkRequest) -> AppResult<PreflightReport> {
        let request = self.validate(request)?;
        let env = environment::snapshot(&*self.probe);
        let factory = self.factory(&request);
        let mut estimated_secs: u32 = request
            .kinds
            .iter()
            .map(|k| factory.estimate_secs(*k, &request))
            .sum();
        if request.cooldown {
            let heavy = request.kinds.iter().filter(|k| k.is_heavy()).count() as u32;
            estimated_secs += heavy.saturating_sub(1) * request.profile.cooldown_secs();
        }
        Ok(PreflightReport {
            load_level: load_level(&env),
            warnings: preflight_warnings(&env),
            environment: env,
            estimated_secs,
            estimated_disk_write_bytes: 0,
            estimated_network_bytes: if request.kinds.contains(&BenchmarkKind::Network) {
                request.network_data_profile.max_bytes()
            } else {
                0
            },
            simulated: request.simulate,
        })
    }

    /// Starts a run on a background thread and returns its id. Fails with
    /// `BenchmarkAlreadyRunning` while another run is active.
    pub fn start(
        self: &Arc<Self>,
        request: BenchmarkRequest,
        sink: Arc<dyn ProgressSink>,
    ) -> AppResult<String> {
        let request = self.validate(request)?;
        let mut active = self.active();
        if let Some(run) = active.as_ref()
            && run.handle.as_ref().is_some_and(|h| !h.is_finished())
        {
            return Err(AppError::BenchmarkAlreadyRunning);
        }
        let id = uuid::Uuid::new_v4().to_string();
        let cancel = Arc::new(CancelToken::new(Duration::from_secs(
            request.profile.timeout_secs(),
        )));
        let handle = {
            let this = Arc::clone(self);
            let id = id.clone();
            let cancel = Arc::clone(&cancel);
            thread::Builder::new()
                .name("benchmark-runner".into())
                .spawn(move || this.run_thread(&id, &request, &cancel, &*sink))
                .map_err(|e| {
                    AppError::SystemQueryFailed(format!("cannot start benchmark thread: {e}"))
                })?
        };
        *active = Some(ActiveRun {
            id: id.clone(),
            cancel,
            handle: Some(handle),
        });
        Ok(id)
    }

    /// Requests cancellation. Returns `false` if that run is not active.
    pub fn cancel(&self, run_id: &str) -> bool {
        match self.active().as_ref() {
            Some(run) if run.id == run_id => {
                run.cancel.cancel(CancelReason::User);
                true
            }
            _ => false,
        }
    }

    pub fn skip_cooldown(&self, run_id: &str) -> bool {
        match self.active().as_ref() {
            Some(run) if run.id == run_id => {
                run.cancel.skip_cooldown();
                true
            }
            _ => false,
        }
    }

    /// Cancels any active run and waits (bounded) for its cleanup.
    pub fn shutdown(&self, wait: Duration) {
        let handle = {
            let mut active = self.active();
            active.as_mut().and_then(|run| {
                run.cancel.cancel(CancelReason::Shutdown);
                run.handle.take()
            })
        };
        if let Some(handle) = handle {
            let deadline = Instant::now() + wait;
            while !handle.is_finished() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(20));
            }
            if handle.is_finished() {
                let _ = handle.join();
            } else {
                tracing::warn!("benchmark did not stop within {wait:?} during shutdown");
            }
        }
    }

    fn run_thread(
        &self,
        id: &str,
        request: &BenchmarkRequest,
        cancel: &CancelToken,
        sink: &dyn ProgressSink,
    ) {
        let started = Instant::now();
        tracing::info!(
            run_id = id,
            kinds = ?request.kinds,
            profile = ?request.profile,
            simulated = request.simulate,
            method_version = METHOD_VERSION,
            "benchmark_started"
        );
        let mut reporter = Reporter::new(sink, id, started);
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            self.execute(id, request, cancel, &mut reporter)
        }))
        .unwrap_or_else(|_| {
            tracing::error!(run_id = id, "benchmark runner panicked");
            Outcome {
                phase: BenchmarkPhase::Failed,
                result_id: None,
                error: Some(FailureInfo::from(&AppError::SystemQueryFailed(
                    "The benchmark stopped unexpectedly.".into(),
                ))),
            }
        });
        let duration_ms = started.elapsed().as_millis();
        match outcome.phase {
            BenchmarkPhase::Completed => {
                tracing::info!(run_id = id, duration_ms, "benchmark_completed");
            }
            BenchmarkPhase::Cancelled => {
                tracing::info!(run_id = id, duration_ms, "benchmark_cancelled");
            }
            _ => tracing::warn!(
                run_id = id,
                duration_ms,
                error = ?outcome.error.as_ref().map(|e| &e.message),
                "benchmark_failed"
            ),
        }
        // Release the slot before announcing the end, so the UI can start a
        // new run as soon as it sees `Finished`.
        {
            let mut active = self.active();
            if active.as_ref().is_some_and(|r| r.id == id) {
                *active = None;
            }
        }
        let message = match outcome.phase {
            BenchmarkPhase::Completed => "Benchmark complete".to_owned(),
            BenchmarkPhase::Cancelled => "Benchmark cancelled".to_owned(),
            _ => outcome
                .error
                .as_ref()
                .map_or_else(|| "Benchmark failed".to_owned(), |e| e.message.clone()),
        };
        reporter.set_category(None, 1.0, 0.0);
        reporter.phase(outcome.phase, message);
        reporter.event(&BenchmarkEvent::Finished {
            run_id: id.to_owned(),
            phase: outcome.phase,
            result_id: outcome.result_id,
            error: outcome.error,
        });
    }

    fn execute(
        &self,
        id: &str,
        request: &BenchmarkRequest,
        cancel: &CancelToken,
        reporter: &mut Reporter<'_>,
    ) -> Outcome {
        let started_at_ms = unix_millis();
        let started = Instant::now();
        reporter.phase(BenchmarkPhase::Preparing, "Collecting system information");
        let env_before = environment::snapshot(&*self.probe);
        let mut warnings = preflight_warnings(&env_before);
        let _mode = BenchmarkModeGuard::new(&*self.probe);
        let monitor = InterferenceMonitor::start(Arc::clone(&self.probe));
        let factory = self.factory(request);

        let count = request.kinds.len().max(1) as f32;
        let mut categories: Vec<CategoryResult> = Vec::new();
        let mut previous_heavy = false;
        for (index, &kind) in request.kinds.iter().enumerate() {
            reporter.set_category(Some(kind), index as f32 / count, 1.0 / count);
            if previous_heavy
                && kind.is_heavy()
                && request.cooldown
                && let Err(e) = cooldown(request, cancel, reporter)
            {
                return stopped(cancel, &e);
            }
            previous_heavy = kind.is_heavy();
            monitor.reset();
            let category_started = Instant::now();
            let result = factory.create(kind, request).and_then(|mut bench| {
                let mut ctx = RunContext {
                    request,
                    cancel,
                    reporter,
                    probe: &*self.probe,
                };
                bench.run(&mut ctx)
            });
            let mut category = match result {
                Ok(category) => category,
                Err(e) if cancel.is_cancelled() => return stopped(cancel, &e),
                Err(e) => {
                    tracing::warn!(run_id = id, ?kind, "benchmark category failed: {e}");
                    let mut c = CategoryResult::empty(kind, CategoryStatus::Failed);
                    c.error = Some(FailureInfo::from(&e));
                    c
                }
            };
            category.kind = kind;
            category.duration_ms =
                u64::try_from(category_started.elapsed().as_millis()).unwrap_or(u64::MAX);
            category.interference = monitor.report();
            finalize_category(&mut category);
            for w in &category.warnings {
                tracing::info!(run_id = id, ?kind, code = ?w.code, "benchmark_warning");
            }
            categories.push(category);
        }
        drop(monitor);

        reporter.set_category(None, 1.0, 0.0);
        reporter.phase(BenchmarkPhase::Finalizing, "Calculating results");
        let completed = categories
            .iter()
            .filter(|c| c.status == CategoryStatus::Completed)
            .count();
        if completed == 0 {
            let error = categories
                .iter()
                .find_map(|c| c.error.clone())
                .unwrap_or_else(|| {
                    FailureInfo::from(&AppError::UnsupportedPlatform(
                        "None of the selected benchmarks can run on this system.".into(),
                    ))
                });
            return Outcome {
                phase: BenchmarkPhase::Failed,
                result_id: None,
                error: Some(error),
            };
        }

        let system_score = scoring::system_score(&categories);
        if system_score.as_ref().is_some_and(|s| s.partial) {
            warnings.push(BenchmarkWarning::new(
                WarningCode::PartialBenchmark,
                "Partial Benchmark: not every category contributed to the system score.",
            ));
        }
        if request.simulate {
            warnings.push(BenchmarkWarning::new(
                WarningCode::Simulated,
                "Simulated run: no hardware was measured.",
            ));
        }
        let mut fp = fingerprint(&env_before);
        fp.gpu = hardware_detail(&categories, BenchmarkKind::Gpu);
        fp.disk = hardware_detail(&categories, BenchmarkKind::Disk);
        let record = BenchmarkRecord {
            schema_version: SCHEMA_VERSION,
            method_version: METHOD_VERSION.into(),
            score_version: SCORE_VERSION,
            id: id.to_owned(),
            started_at_ms,
            duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            app_version: env_before.app_version.clone(),
            profile: request.profile,
            kinds: request.kinds.clone(),
            simulated: request.simulate,
            environment_after: Some(environment::snapshot(&*self.probe)),
            environment_before: env_before,
            fingerprint: fp,
            categories,
            system_score,
            warnings,
        };
        let result_id = if self.save_history.load(Ordering::SeqCst) {
            match self.history.save(&record) {
                Ok(()) => Some(record.id.clone()),
                Err(e) => {
                    tracing::warn!(run_id = id, "could not save benchmark result: {e}");
                    None
                }
            }
        } else {
            None
        };
        Outcome {
            phase: BenchmarkPhase::Completed,
            result_id,
            error: None,
        }
    }
}

/// Scores, confidence and measurement-derived warnings for one category.
pub fn finalize_category(category: &mut CategoryResult) {
    scoring::score_category(category);
    let derived = confidence::derived_warnings(category);
    category.warnings.extend(derived);
    category.confidence = confidence::assess(category);
}

/// The `hardware` detail a category reports (adapter name, disk model).
fn hardware_detail(categories: &[CategoryResult], kind: BenchmarkKind) -> Option<String> {
    categories
        .iter()
        .find(|c| c.kind == kind && c.status == CategoryStatus::Completed)?
        .details
        .iter()
        .find(|d| d.key == "hardware")
        .map(|d| d.value.clone())
}

fn cooldown(
    request: &BenchmarkRequest,
    cancel: &CancelToken,
    reporter: &mut Reporter<'_>,
) -> AppResult<()> {
    let secs = request.profile.cooldown_secs();
    if secs == 0 {
        return Ok(());
    }
    cancel.take_skip_cooldown();
    reporter.phase(BenchmarkPhase::CoolingDown, "Cooling down before next test");
    let end = Instant::now() + Duration::from_secs(u64::from(secs));
    while Instant::now() < end {
        if cancel.take_skip_cooldown() {
            break;
        }
        let left = end.saturating_duration_since(Instant::now()).as_secs();
        reporter.progress(0.0, format!("Cooling down before next test ({left}s)"));
        cancel.sleep(Duration::from_millis(250))?;
    }
    Ok(())
}

fn stopped(cancel: &CancelToken, error: &AppError) -> Outcome {
    match cancel.reason() {
        Some(CancelReason::Timeout) => Outcome {
            phase: BenchmarkPhase::Failed,
            result_id: None,
            error: Some(FailureInfo::from(error)),
        },
        _ => Outcome {
            phase: BenchmarkPhase::Cancelled,
            result_id: None,
            error: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmarks::environment::tests::FakeProbe;
    use crate::benchmarks::history::tests::temp_dir;
    use crate::benchmarks::mock::{MockFactory, MockOutcome};
    use crate::benchmarks::models::{BenchmarkProfile, NetworkDataProfile};
    use crate::benchmarks::progress::tests::RecordingSink;
    use crate::errors::ErrorKind;

    fn request(kinds: Vec<BenchmarkKind>) -> BenchmarkRequest {
        BenchmarkRequest {
            kinds,
            profile: BenchmarkProfile::Quick,
            cpu_workers: None,
            gpu_adapter_id: None,
            disk_mount_point: None,
            disk_allow_write: false,
            network_data_profile: NetworkDataProfile::DataSaver,
            sustained_secs: None,
            cooldown: false,
            simulate: true,
        }
    }

    struct Setup {
        coordinator: Arc<BenchmarkCoordinator>,
        probe: Arc<FakeProbe>,
        dir: std::path::PathBuf,
    }

    fn setup(factory: MockFactory) -> Setup {
        let probe = FakeProbe::new(3.0);
        let dir = temp_dir("coordinator");
        let factory: Arc<dyn BenchmarkFactory> = Arc::new(factory);
        let coordinator = BenchmarkCoordinator::new(
            probe.clone(),
            Arc::clone(&factory),
            factory,
            Arc::new(HistoryStore::new(Some(dir.clone()))),
            true,
        );
        Setup {
            coordinator,
            probe,
            dir,
        }
    }

    fn finished(
        sink: &RecordingSink,
    ) -> Option<(BenchmarkPhase, Option<String>, Option<FailureInfo>)> {
        sink.events.lock().unwrap().iter().find_map(|e| match e {
            BenchmarkEvent::Finished {
                phase,
                result_id,
                error,
                ..
            } => Some((*phase, result_id.clone(), error.clone())),
            _ => None,
        })
    }

    fn wait_finished(
        sink: &RecordingSink,
    ) -> (BenchmarkPhase, Option<String>, Option<FailureInfo>) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            if let Some(f) = finished(sink) {
                return f;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("run did not finish");
    }

    #[test]
    fn completed_run_is_scored_and_persisted() {
        let s = setup(MockFactory::new(Duration::from_millis(1)));
        let sink = Arc::new(RecordingSink::default());
        let id = s
            .coordinator
            .start(
                request(vec![
                    BenchmarkKind::Disk,
                    BenchmarkKind::Cpu,
                    BenchmarkKind::Gpu,
                ]),
                sink.clone(),
            )
            .unwrap();
        let (phase, result_id, error) = wait_finished(&sink);
        assert_eq!(phase, BenchmarkPhase::Completed);
        assert!(error.is_none());
        assert_eq!(result_id.as_deref(), Some(id.as_str()));

        let record = s.coordinator.history().get(&id).unwrap();
        assert_eq!(record.method_version, METHOD_VERSION);
        assert!(record.simulated);
        // Categories run in canonical order regardless of request order.
        let kinds: Vec<_> = record.categories.iter().map(|c| c.kind).collect();
        assert_eq!(
            kinds,
            vec![BenchmarkKind::Cpu, BenchmarkKind::Gpu, BenchmarkKind::Disk]
        );
        assert!(record.categories.iter().all(|c| c.score.is_some()));
        assert!(record.system_score.as_ref().is_some_and(|s| !s.partial));
        assert!(!record.categories[0].metrics[0].samples.is_empty());

        // Benchmark Mode was switched on and off again.
        assert!(!s.probe.mode.load(Ordering::SeqCst));
        assert_eq!(s.coordinator.running_run_id(), None);

        // Phases only move forward and end in a terminal state.
        let phases: Vec<_> = sink.progress().iter().map(|p| p.phase).collect();
        assert_eq!(phases.first(), Some(&BenchmarkPhase::Preparing));
        assert_eq!(phases.last(), Some(&BenchmarkPhase::Completed));
        assert!(phases.contains(&BenchmarkPhase::Finalizing));
        std::fs::remove_dir_all(s.dir).unwrap();
    }

    #[test]
    fn second_run_is_rejected_while_one_is_active() {
        let s = setup(MockFactory::new(Duration::from_millis(50)));
        let sink = Arc::new(RecordingSink::default());
        let id = s
            .coordinator
            .start(request(vec![BenchmarkKind::Cpu]), sink.clone())
            .unwrap();
        let err = s
            .coordinator
            .start(
                request(vec![BenchmarkKind::Gpu]),
                Arc::new(RecordingSink::default()),
            )
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::BenchmarkAlreadyRunning);
        assert!(s.coordinator.cancel(&id));
        wait_finished(&sink);
        std::fs::remove_dir_all(s.dir).unwrap();
    }

    #[test]
    fn cancellation_stops_promptly_and_writes_no_history() {
        let s = setup(MockFactory::new(Duration::from_millis(200)));
        let sink = Arc::new(RecordingSink::default());
        let id = s
            .coordinator
            .start(
                request(vec![BenchmarkKind::Cpu, BenchmarkKind::Disk]),
                sink.clone(),
            )
            .unwrap();
        thread::sleep(Duration::from_millis(300));
        let cancelled_at = Instant::now();
        assert!(s.coordinator.cancel(&id));
        assert!(!s.coordinator.cancel("someone-else"));
        let (phase, result_id, _) = wait_finished(&sink);
        assert!(cancelled_at.elapsed() < Duration::from_secs(2));
        assert_eq!(phase, BenchmarkPhase::Cancelled);
        assert!(result_id.is_none());
        assert!(s.coordinator.history().list().is_empty());
        assert!(!s.probe.mode.load(Ordering::SeqCst), "monitoring restored");
        // The slot is free again.
        let sink2 = Arc::new(RecordingSink::default());
        s.coordinator
            .start(request(vec![BenchmarkKind::Network]), sink2.clone())
            .unwrap();
        assert_eq!(wait_finished(&sink2).0, BenchmarkPhase::Completed);
        std::fs::remove_dir_all(s.dir).unwrap();
    }

    #[test]
    fn failed_and_unsupported_categories_do_not_abort_the_suite() {
        let factory = MockFactory::new(Duration::from_millis(1))
            .with(BenchmarkKind::Gpu, MockOutcome::Fail("no device".into()))
            .with(BenchmarkKind::Disk, MockOutcome::Unsupported("nope".into()));
        let s = setup(factory);
        let sink = Arc::new(RecordingSink::default());
        let id = s
            .coordinator
            .start(
                request(vec![
                    BenchmarkKind::Cpu,
                    BenchmarkKind::Gpu,
                    BenchmarkKind::Disk,
                ]),
                sink.clone(),
            )
            .unwrap();
        assert_eq!(wait_finished(&sink).0, BenchmarkPhase::Completed);
        let record = s.coordinator.history().get(&id).unwrap();
        let gpu = record.category(BenchmarkKind::Gpu).unwrap();
        assert_eq!(gpu.status, CategoryStatus::Failed);
        assert_eq!(
            gpu.error.as_ref().unwrap().kind,
            ErrorKind::GpuInitializationFailed
        );
        assert_eq!(gpu.score, None);
        assert_eq!(
            record.category(BenchmarkKind::Disk).unwrap().status,
            CategoryStatus::Unsupported
        );
        // Only one scored category → no system score (never zero-filled).
        assert!(record.system_score.is_none());
        std::fs::remove_dir_all(s.dir).unwrap();
    }

    #[test]
    fn run_with_nothing_completed_fails_without_history() {
        let factory = MockFactory::new(Duration::from_millis(1)).with(
            BenchmarkKind::Gpu,
            MockOutcome::Unsupported("no gpu".into()),
        );
        let s = setup(factory);
        let sink = Arc::new(RecordingSink::default());
        s.coordinator
            .start(request(vec![BenchmarkKind::Gpu]), sink.clone())
            .unwrap();
        let (phase, result_id, error) = wait_finished(&sink);
        assert_eq!(phase, BenchmarkPhase::Failed);
        assert!(result_id.is_none());
        assert!(error.is_some());
        assert!(s.coordinator.history().list().is_empty());
        std::fs::remove_dir_all(s.dir).unwrap();
    }

    #[test]
    fn cooldown_runs_between_heavy_categories_and_can_be_skipped() {
        let s = setup(MockFactory::new(Duration::from_millis(1)));
        let sink = Arc::new(RecordingSink::default());
        let mut req = request(vec![BenchmarkKind::Cpu, BenchmarkKind::Gpu]);
        req.profile = BenchmarkProfile::Standard; // 10 s cooldown
        req.cooldown = true;
        let id = s.coordinator.start(req, sink.clone()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !sink
            .progress()
            .iter()
            .any(|p| p.phase == BenchmarkPhase::CoolingDown)
        {
            assert!(Instant::now() < deadline, "no cooldown");
            thread::sleep(Duration::from_millis(10));
        }
        let skipped_at = Instant::now();
        assert!(s.coordinator.skip_cooldown(&id));
        assert_eq!(wait_finished(&sink).0, BenchmarkPhase::Completed);
        assert!(skipped_at.elapsed() < Duration::from_secs(5));
        std::fs::remove_dir_all(s.dir).unwrap();
    }

    #[test]
    fn history_can_be_disabled() {
        let s = setup(MockFactory::new(Duration::from_millis(1)));
        s.coordinator.set_save_history(false);
        let sink = Arc::new(RecordingSink::default());
        s.coordinator
            .start(request(vec![BenchmarkKind::Cpu]), sink.clone())
            .unwrap();
        let (phase, result_id, _) = wait_finished(&sink);
        assert_eq!(phase, BenchmarkPhase::Completed);
        assert!(result_id.is_none());
        assert!(s.coordinator.history().list().is_empty());
        std::fs::remove_dir_all(s.dir).unwrap();
    }

    #[test]
    fn shutdown_cancels_and_waits() {
        let s = setup(MockFactory::new(Duration::from_millis(500)));
        let sink = Arc::new(RecordingSink::default());
        s.coordinator
            .start(request(vec![BenchmarkKind::Cpu]), sink.clone())
            .unwrap();
        thread::sleep(Duration::from_millis(50));
        s.coordinator.shutdown(Duration::from_secs(5));
        assert_eq!(
            finished(&sink).map(|f| f.0),
            Some(BenchmarkPhase::Cancelled)
        );
        std::fs::remove_dir_all(s.dir).unwrap();
    }

    #[test]
    fn preflight_reports_load_and_estimates() {
        let s = setup(MockFactory::new(Duration::from_millis(100)));
        *s.probe.cpu.lock().unwrap() = 40.0;
        let mut req = request(vec![BenchmarkKind::Cpu, BenchmarkKind::Network]);
        req.network_data_profile = NetworkDataProfile::Standard;
        let report = s.coordinator.preflight(req).unwrap();
        assert_eq!(report.load_level, super::super::models::LoadLevel::High);
        assert!(!report.warnings.is_empty());
        assert!(report.estimated_secs > 0);
        assert_eq!(report.estimated_network_bytes, 200_000_000);
        std::fs::remove_dir_all(s.dir).unwrap();
    }
}
