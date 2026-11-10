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

fn quick(c: &Coordinator, sink: &Arc<Recorder>, cfg: CpuRunConfig) -> AppResult<BenchmarkStatus> {
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

fn target(dir: &std::path::Path) -> DiskTargetInfo {
    DiskTargetInfo {
        id: "test".into(),
        label: "Test folder".into(),
        directory: dir.to_string_lossy().into_owned(),
        mount_point: "/".into(),
        file_system: "ext4".into(),
        device: None,
        free_bytes: 100 << 30,
        total_bytes: 200 << 30,
        recommended: true,
        notes: vec![],
    }
}

// Unix only: the disk engine does not exist on other platforms yet.
#[cfg(unix)]
#[test]
fn a_disk_run_is_saved_with_its_target_mode_and_wear() {
    let dir = tempfile::tempdir().unwrap();
    let t = target(dir.path());
    let c = coordinator().with_disk_targets(vec![t.clone()]);
    let sink = Arc::new(Recorder::default());
    let sink2: Arc<dyn ProgressSink> = sink.clone();
    c.start_with(
        BenchmarkKind::Disk,
        BenchmarkProfile::Quick,
        RunPlan::Disk {
            config: Some(DiskConfig::test()),
            target: Box::new(t),
        },
        sink2,
    )
    .expect("starts");
    let done = wait_until_idle(&c);
    assert_eq!(done.state, BenchmarkState::Completed, "{:?}", done.message);
    let results = lock(&sink.results);
    let r = &results[0];
    let d = r.disk.as_ref().expect("disk result");
    assert_eq!(d.phases.len(), 6);
    assert_eq!(d.target.id, "test");
    assert!(d.bytes_written > 0 && d.bytes_read > 0);
    assert!(r.method.method_id.starts_with("disk-"));
    assert!(r.method.method_id.ends_with(match d.io_mode {
        crate::benchmarks::model::IoMode::Direct => "direct-v1",
        crate::benchmarks::model::IoMode::Buffered => "buffered-v1",
    }));
    assert!(r.id.starts_with("disk-"));
    assert_eq!(c.history().len(), 1);
    assert!(
        !dir.path()
            .join(crate::benchmarks::disk::file::DIR_NAME)
            .exists(),
        "no test file left behind"
    );
}

#[test]
fn disk_and_internet_runs_refuse_unknown_targets_and_missing_consent() {
    let c = coordinator().with_disk_targets(vec![]);
    let sink: Arc<dyn ProgressSink> = Arc::new(Recorder::default());
    let go =
        |options: StartOptions, kind| c.start(kind, BenchmarkProfile::Quick, options, sink.clone());
    assert!(matches!(
        go(StartOptions::default(), BenchmarkKind::Disk),
        Err(AppError::InvalidArgument(_))
    ));
    assert!(matches!(
        go(
            StartOptions {
                disk_target: Some("/etc".into()),
                ..Default::default()
            },
            BenchmarkKind::Disk
        ),
        Err(AppError::InvalidArgument(_))
    ));
    assert!(matches!(
        go(StartOptions::default(), BenchmarkKind::Internet),
        Err(AppError::InvalidArgument(_))
    ));
    // Nothing started, so the lock is free.
    assert!(lock(&c.shared.active).is_none());
    assert_eq!(c.status().state, BenchmarkState::Idle);
}

#[test]
fn disk_plan_reports_the_write_volume_or_the_reason_it_is_blocked() {
    let dir = tempfile::tempdir().unwrap();
    let t = target(dir.path());
    let c = coordinator().with_disk_targets(vec![t]);
    let p = c.disk_plan("test", BenchmarkProfile::Standard).unwrap();
    assert!(p.blocked.is_none() || p.file_bytes == 0);
    if p.blocked.is_none() {
        assert!(p.estimated_bytes_written > p.file_bytes);
    }
    assert!(c.disk_plan("nope", BenchmarkProfile::Quick).is_err());
}

#[test]
fn an_internet_run_with_consent_is_saved_without_a_score() {
    use crate::benchmarks::internet::provider::{Cloudflare, test_server};
    let server = test_server::start(test_server::Mode::Normal, 2_500_000);
    let c = coordinator().with_internet_provider(Arc::new(Cloudflare::with_base(
        &server.base,
        Duration::from_secs(5),
    )));
    let sink = Arc::new(Recorder::default());
    let sink2: Arc<dyn ProgressSink> = sink.clone();
    c.start_with(
        BenchmarkKind::Internet,
        BenchmarkProfile::Quick,
        RunPlan::Internet {
            config: Some(InternetConfig::test()),
        },
        sink2,
    )
    .expect("starts");
    let done = wait_until_idle(&c);
    assert_eq!(done.state, BenchmarkState::Completed, "{:?}", done.message);
    let results = lock(&sink.results);
    let r = &results[0];
    let n = r.internet.as_ref().expect("internet result");
    assert!(n.download.as_ref().unwrap().median_mbps > 10.0);
    assert!(r.scores.is_none() && r.disk_score.is_none() && r.gpu_score.is_none());
    assert_eq!(r.method.method_id, "internet-cloudflare-v1");
    assert!(r.id.starts_with("net-"));
    assert_eq!(c.history().len(), 1);
}

#[test]
fn internet_tests_have_a_cooldown_and_a_session_data_budget() {
    use crate::benchmarks::internet::provider::{Cloudflare, test_server};
    let server = test_server::start(test_server::Mode::Normal, 0);
    let sink: Arc<dyn ProgressSink> = Arc::new(Recorder::default());
    let provider = Arc::new(Cloudflare::with_base(&server.base, Duration::from_secs(5)));
    let go = |c: &Coordinator| {
        c.start(
            BenchmarkKind::Internet,
            BenchmarkProfile::Quick,
            StartOptions {
                internet_consent: true,
                ..Default::default()
            },
            sink.clone(),
        )
    };

    // Cooldown: a second start right after the first is refused.
    let c = coordinator()
        .with_internet_provider(provider.clone())
        .with_internet_limits(Duration::from_secs(300), u64::MAX);
    assert!(go(&c).is_ok());
    c.cancel();
    wait_until_idle(&c);
    let e = go(&c).expect_err("cooling down");
    assert!(e.to_string().contains("more seconds"), "{e}");

    // Budget: a plan that does not fit what is left is refused up front.
    let c = coordinator()
        .with_internet_provider(provider)
        .with_internet_limits(Duration::ZERO, 100_000_000);
    let e = go(&c).expect_err("over budget");
    assert!(e.to_string().contains("allowance"), "{e}");
    assert_eq!(c.status().state, BenchmarkState::Idle);
}
