//! Runs one benchmark at a time and owns everything around it: the lock,
//! the state machine, rate-limited progress, cancellation, saving, and
//! cleanup on every exit path (success, failure, cancel, panic).

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::cancel::CancelToken;
use super::cpu::runner::{CpuHooks, CpuRunConfig, Observer, run_cpu};
use super::disk::engine::run_disk;
use super::disk::plan::{self as disk_plan_rules, DiskConfig};
use super::disk::{self, DiskRequest};
use super::gpu::runner::{GpuRunConfig, run_gpu};
use super::gpu::wgpu_backend::WgpuBackend;
use super::gpu::{self, GpuBackend};
use super::history::HistoryStore;
use super::internet::provider::{Cloudflare, Provider};
use super::internet::{self, InternetConfig, run_internet};
use super::model::{
    BenchmarkKind, BenchmarkProfile, BenchmarkResult, BenchmarkState, BenchmarkStatus, DiskPlan,
    DiskResult, DiskTargetInfo, GpuAdapterInfo, GpuDeviceType, GpuResult, InternetPlan,
    LiveReading, NetworkPath,
};
use super::{BenchmarkError, environment, scoring};
use crate::errors::{AppError, AppResult};
use crate::models::TemperatureReading;
use crate::monitoring::{Engine, unix_millis};
use crate::platform;
use crate::sync_util::lock;

/// Least time between two internet tests. The public speed-test endpoints
/// limit repeated large transfers per address (HTTP 429 for about an hour), so
/// back-to-back runs would only fail.
const INTERNET_COOLDOWN: Duration = Duration::from_secs(120);

/// Most test data one app session may move in total, both directions.
const INTERNET_SESSION_BUDGET_BYTES: u64 = 1_000_000_000;

/// Other drive traffic above this when a disk test starts earns a warning.
const DISK_BUSY_WARN_BYTES_PER_SEC: f64 = 20e6;

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

pub struct Coordinator {
    engine: Arc<Engine>,
    history: Arc<HistoryStore>,
    shared: Arc<Shared>,
    gpu: Arc<dyn GpuBackend>,
    provider: Arc<dyn Provider>,
    /// When the last internet test started, for the cooldown.
    internet_last: Arc<Mutex<Option<Instant>>>,
    /// Bytes moved by internet tests since the app started.
    internet_used: Arc<AtomicU64>,
    internet_cooldown: Duration,
    internet_budget: u64,
    /// Tests replace the list built from the mounted filesystems.
    targets_override: Option<Vec<DiskTargetInfo>>,
    /// Tests run unoptimised; this lets them exercise scoring and saving.
    score_development_builds: bool,
}

/// What the user chose for a run. Everything is an id or a flag; the backend
/// resolves ids to paths and addresses.
#[derive(Debug, Clone, Default)]
pub struct StartOptions {
    /// Graphics adapter id (GPU runs).
    pub adapter: Option<String>,
    /// Disk location id from [`Coordinator::disk_targets`] (disk runs).
    pub disk_target: Option<String>,
    /// The user agreed to send and receive test data (internet runs).
    pub internet_consent: bool,
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
    Disk {
        config: Option<DiskConfig>,
        target: Box<DiskTargetInfo>,
    },
    Internet {
        config: Option<InternetConfig>,
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
            provider: Arc::new(Cloudflare::new()),
            internet_last: Arc::new(Mutex::new(None)),
            internet_used: Arc::new(AtomicU64::new(0)),
            internet_cooldown: INTERNET_COOLDOWN,
            internet_budget: INTERNET_SESSION_BUDGET_BYTES,
            targets_override: None,
            score_development_builds: false,
        }
    }

    /// Replaces the internet provider (tests use a local server).
    #[doc(hidden)]
    pub fn with_internet_provider(mut self, provider: Arc<dyn Provider>) -> Self {
        self.provider = provider;
        self
    }

    /// Replaces the internet cooldown and session budget (tests).
    #[doc(hidden)]
    pub fn with_internet_limits(mut self, cooldown: Duration, budget_bytes: u64) -> Self {
        self.internet_cooldown = cooldown;
        self.internet_budget = budget_bytes;
        self
    }

    /// Replaces the disk locations (tests use temporary folders).
    #[doc(hidden)]
    pub fn with_disk_targets(mut self, targets: Vec<DiskTargetInfo>) -> Self {
        self.targets_override = Some(targets);
        self
    }

    /// Locations the disk benchmark may use, from the mounted filesystems.
    pub fn disk_targets(&self) -> Vec<DiskTargetInfo> {
        if let Some(t) = &self.targets_override {
            return t.clone();
        }
        let Some(tick) = self.engine.latest() else {
            return Vec::new();
        };
        let mounts: Vec<disk::targets::MountInfo> = tick
            .disks
            .iter()
            .map(|d| disk::targets::MountInfo {
                mount_point: d.mount_point.clone(),
                file_system: d.file_system.clone(),
                device: d.name.clone(),
                total_bytes: d.total_bytes,
                free_bytes: d.available_bytes,
                read_only: d.read_only,
            })
            .collect();
        let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
        disk::targets::build_targets(&mounts, home.as_deref(), &std::env::temp_dir(), &|p| {
            disk::writable(p)
        })
    }

    /// What a disk run on `target_id` would write, or why it cannot happen.
    pub fn disk_plan(&self, target_id: &str, profile: BenchmarkProfile) -> AppResult<DiskPlan> {
        let target = self.find_target(target_id)?;
        let cfg = DiskConfig::for_profile(profile);
        let free = disk::free_space(std::path::Path::new(&target.directory), target.free_bytes);
        let planned = disk_plan_rules::plan_file_bytes(cfg.file_bytes, cfg.min_file_bytes, free);
        let file_bytes = planned.clone().unwrap_or(0);
        Ok(DiskPlan {
            target_id: target.id,
            directory: target.directory,
            file_bytes,
            estimated_bytes_written: disk_plan_rules::estimated_bytes_written(&cfg, file_bytes),
            estimated_seconds: disk_plan_rules::estimated_seconds(profile),
            blocked: planned.err(),
        })
    }

    /// What an internet run would send and receive, and to whom.
    pub fn internet_plan(&self, profile: BenchmarkProfile) -> InternetPlan {
        internet::plan(&*self.provider, profile)
    }

    fn find_target(&self, id: &str) -> AppResult<DiskTargetInfo> {
        self.disk_targets()
            .into_iter()
            .find(|t| t.id == id)
            .ok_or_else(|| AppError::InvalidArgument("That location is not available.".into()))
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

    /// `options` carries ids and flags only; options that do not apply to the
    /// kind are ignored.
    pub fn start(
        &self,
        kind: BenchmarkKind,
        profile: BenchmarkProfile,
        options: StartOptions,
        sink: Arc<dyn ProgressSink>,
    ) -> AppResult<BenchmarkStatus> {
        let plan = match kind {
            BenchmarkKind::Cpu => {
                let logical = self.engine.static_info().logical_core_count as usize;
                RunPlan::Cpu(CpuRunConfig::for_profile(profile, logical))
            }
            BenchmarkKind::Gpu => RunPlan::Gpu {
                config: None,
                adapter: options.adapter,
            },
            BenchmarkKind::Disk => {
                let id = options
                    .disk_target
                    .ok_or_else(|| AppError::InvalidArgument("Choose a location first.".into()))?;
                RunPlan::Disk {
                    config: None,
                    target: Box::new(self.find_target(&id)?),
                }
            }
            BenchmarkKind::Internet => {
                // Checked here, not only in the UI: nothing leaves the
                // machine unless the caller says the user agreed.
                if !options.internet_consent {
                    return Err(AppError::InvalidArgument(
                        "Agree to send and receive test data before running the internet test."
                            .into(),
                    ));
                }
                self.check_internet_allowed(profile)?;
                RunPlan::Internet { config: None }
            }
        };
        let started = self.start_with(kind, profile, plan, sink)?;
        if kind == BenchmarkKind::Internet {
            *lock(&self.internet_last) = Some(Instant::now());
        }
        Ok(started)
    }

    /// Cooldown and session data budget for internet tests.
    fn check_internet_allowed(&self, profile: BenchmarkProfile) -> AppResult<()> {
        if let Some(last) = *lock(&self.internet_last) {
            let waited = last.elapsed();
            if waited < self.internet_cooldown {
                let left = (self.internet_cooldown - waited).as_secs() + 1;
                return Err(AppError::InvalidArgument(format!(
                    "Wait about {left} more seconds before running the internet test again. The test server limits repeated transfers."
                )));
            }
        }
        let plan = self.internet_plan(profile);
        let used = self.internet_used.load(Ordering::Relaxed);
        if used + plan.max_download_bytes + plan.max_upload_bytes > self.internet_budget {
            return Err(AppError::InvalidArgument(format!(
                "This session has used {} MB of its {} MB internet test allowance. Restart the app to run it again.",
                used / 1_000_000,
                self.internet_budget / 1_000_000
            )));
        }
        Ok(())
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
            provider: Arc::clone(&self.provider),
            internet_used: Arc::clone(&self.internet_used),
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
    provider: Arc<dyn Provider>,
    internet_used: Arc<AtomicU64>,
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

    /// Combined read and write rate of all drives right now, in bytes/s.
    fn other_disk_activity(&self) -> Option<f64> {
        let tick = self.engine.latest()?;
        Some(
            tick.disks
                .iter()
                .map(|d| d.read_bytes_per_sec.unwrap_or(0.0) + d.write_bytes_per_sec.unwrap_or(0.0))
                .sum(),
        )
    }

    /// The interface carrying internet traffic and whether a tunnel is up.
    fn network_path(&self) -> NetworkPath {
        let nets = self
            .engine
            .latest()
            .map(|t| t.networks.clone())
            .unwrap_or_default();
        let kind_of = |name: &str| {
            nets.iter()
                .find(|n| n.name == name)
                .map_or(crate::models::InterfaceKind::Unknown, |n| n.kind)
        };
        let others: Vec<_> = nets.iter().map(|n| (n.name.clone(), n.kind)).collect();
        internet::route::describe_path(internet::route::read_default_route(), &kind_of, &others)
    }

    fn execute(&self, reporter: &Reporter) -> Result<BenchmarkResult, BenchmarkError> {
        let started_ms = unix_millis();
        let started = Instant::now();
        reporter.set_state(
            BenchmarkState::Preflight,
            match self.plan {
                RunPlan::Cpu(_) => "Checking the system",
                RunPlan::Gpu { .. } => "Checking graphics adapters",
                RunPlan::Disk { .. } => "Checking the drive",
                RunPlan::Internet { .. } => "Checking the connection",
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
            environment: env,
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
            RunPlan::Cpu(cfg) => self.execute_cpu(cfg, reporter, &mut warnings, &mut result)?,
            RunPlan::Gpu { config, adapter } => self.execute_gpu(
                config.as_ref(),
                adapter.as_deref(),
                reporter,
                &mut warnings,
                &mut result,
            )?,
            RunPlan::Disk { config, target } => self.execute_disk(
                config.as_ref(),
                target,
                reporter,
                &mut warnings,
                &mut result,
            )?,
            RunPlan::Internet { config } => {
                self.execute_internet(config.as_ref(), reporter, &mut warnings, &mut result)?
            }
        }
        result.warnings = warnings;
        result.duration_ms = started.elapsed().as_millis() as u64;
        Ok(result)
    }

    fn execute_cpu(
        &self,
        cfg: &CpuRunConfig,
        reporter: &Reporter,
        warnings: &mut Vec<String>,
        result: &mut BenchmarkResult,
    ) -> Result<(), BenchmarkError> {
        let development_build = result.environment.development_build;
        let hooks = EngineHooks(&self.engine);
        let out = run_cpu(cfg, &hooks, &self.cancel, reporter)?;
        reporter.set_state(BenchmarkState::Finalizing, "Calculating results");
        let scores = if development_build {
            None
        } else {
            scoring::cpu_scores(&out.workloads)
        };
        if scores.is_none() && !development_build {
            warnings.push("Some measurements were unavailable, so no score was calculated.".into());
        }
        result.stability = scoring::stability(&out.workloads, warnings);
        result.workloads = out.workloads;
        result.sustained = out.sustained;
        result.scores = scores;
        result.affinity = out.affinity;
        Ok(())
    }

    fn execute_gpu(
        &self,
        config: Option<&GpuRunConfig>,
        adapter: Option<&str>,
        reporter: &Reporter,
        warnings: &mut Vec<String>,
        result: &mut BenchmarkResult,
    ) -> Result<(), BenchmarkError> {
        let development_build = result.environment.development_build;
        let profile = self.profile;
        let (info, mut exec, cfg) = self.gpu.open(adapter, &|a| {
            config.cloned().unwrap_or_else(|| {
                GpuRunConfig::for_profile(profile, a.device_type == GpuDeviceType::Discrete)
            })
        })?;
        let out = run_gpu(&mut *exec, &cfg, &self.cancel, reporter)?;
        // Release the device before the (possibly slow) bookkeeping.
        drop(exec);
        reporter.set_state(BenchmarkState::Finalizing, "Calculating results");
        let software = info.device_type == GpuDeviceType::Software;
        let score = if development_build || software {
            None
        } else {
            scoring::gpu_score(&out.workloads)
        };
        if score.is_none() && !development_build && !software {
            warnings.push("Some measurements were unavailable, so no score was calculated.".into());
        }
        result.method = scoring::gpu_method();
        result.stability = scoring::gpu_stability(&out.workloads, warnings);
        result.gpu_score = score;
        result.sustained = None;
        result.gpu = Some(GpuResult {
            notes: gpu::notes(&info, out.timing_method),
            adapter: info,
            timing_method: out.timing_method,
            workloads: out.workloads,
            sustained: out.sustained,
        });
        Ok(())
    }

    fn execute_disk(
        &self,
        config: Option<&DiskConfig>,
        target: &DiskTargetInfo,
        reporter: &Reporter,
        warnings: &mut Vec<String>,
        result: &mut BenchmarkResult,
    ) -> Result<(), BenchmarkError> {
        let cfg = config
            .cloned()
            .unwrap_or_else(|| DiskConfig::for_profile(self.profile));
        let parent = std::path::PathBuf::from(&target.directory);
        disk::remove_stale(&parent);
        let busy = self.other_disk_activity();
        let req = DiskRequest {
            free_bytes: disk::free_space(&parent, target.free_bytes),
            parent,
            run_id: self.id.clone(),
            config: cfg,
            want_direct: true,
        };
        let out = run_disk(&req, &self.cancel, reporter)?;
        reporter.set_state(BenchmarkState::Finalizing, "Calculating results");
        let mut notes = out.io_mode_note.iter().cloned().collect::<Vec<_>>();
        notes.extend(target.notes.iter().cloned());
        if out.io_mode == super::model::IoMode::Buffered {
            warnings.push(
                "Buffered I/O was used, so the operating system's cache is part of what was measured. No score is given."
                    .into(),
            );
        }
        if let Some(rate) = busy.filter(|r| *r > DISK_BUSY_WARN_BYTES_PER_SEC) {
            warnings.push(format!(
                "Other programs were using the drive ({:.0} MB/s) when the test started, which can lower the results.",
                rate / 1e6
            ));
        }
        let score = if result.environment.development_build {
            None
        } else {
            scoring::disk_score(&out.phases, out.io_mode)
        };
        result.method = scoring::disk_method(out.io_mode);
        result.stability = scoring::disk_stability(&out.phases, warnings);
        result.disk_score = score;
        result.disk = Some(DiskResult {
            device: target.device.as_deref().and_then(disk::device::device_info),
            target: target.clone(),
            io_mode: out.io_mode,
            io_mode_note: out.io_mode_note,
            file_bytes: out.file_bytes,
            bytes_written: out.bytes_written,
            bytes_read: out.bytes_read,
            phases: out.phases,
            notes,
        });
        Ok(())
    }

    fn execute_internet(
        &self,
        config: Option<&InternetConfig>,
        reporter: &Reporter,
        warnings: &mut Vec<String>,
        result: &mut BenchmarkResult,
    ) -> Result<(), BenchmarkError> {
        let cfg = config
            .cloned()
            .unwrap_or_else(|| InternetConfig::for_profile(self.profile));
        let out = run_internet(
            &*self.provider,
            &cfg,
            self.network_path(),
            &self.cancel,
            &self.internet_used,
            reporter,
        )?;
        reporter.set_state(BenchmarkState::Finalizing, "Calculating results");
        let dl = out.download.as_ref().map(|t| t.samples.clone());
        let ul = out.upload.as_ref().map(|t| t.samples.clone());
        result.method = scoring::internet_method();
        result.stability = scoring::internet_stability(
            dl.as_deref().unwrap_or(&[]),
            ul.as_deref().unwrap_or(&[]),
            warnings,
        );
        if out.path.vpn_suspected {
            warnings.push("A VPN or tunnel looks active; results include its overhead.".into());
        }
        result.internet = Some(out);
        Ok(())
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
mod tests;
