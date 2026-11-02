//! System state around a benchmark: the pre-run snapshot, background-load
//! assessment, hardware fingerprint and the interference monitor.
//!
//! Everything here reads the monitoring engine's published snapshots; the
//! benchmark never polls the OS for metrics itself, so monitoring and
//! benchmarking share one source of truth and one polling cost.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use super::models::{
    BenchmarkWarning, EnvironmentSnapshot, HardwareFingerprint, InterferenceReport, LoadLevel,
    PowerSource, PowerState, WarningCode,
};
use crate::models::{HistoryPoint, MetricsTick, StaticSystemInfo};
use crate::monitoring::{Engine, unix_millis};

/// CPU use by other applications at which the pre-run check warns.
pub const ELEVATED_LOAD_PERCENT: f32 = 15.0;
/// …and at which it recommends waiting.
pub const HIGH_LOAD_PERCENT: f32 = 30.0;
/// Window averaged by the pre-run load check.
const LOAD_WINDOW_SECS: u32 = 5;

/// Read access to the monitoring engine (a trait so tests can fake it).
pub trait SystemProbe: Send + Sync {
    fn static_info(&self) -> StaticSystemInfo;
    fn latest(&self) -> Option<Arc<MetricsTick>>;
    fn recent(&self, seconds: u32) -> Vec<HistoryPoint>;
    fn set_benchmark_mode(&self, active: bool);
    fn power(&self) -> PowerState {
        PowerState::default()
    }
}

impl SystemProbe for Engine {
    fn static_info(&self) -> StaticSystemInfo {
        Engine::static_info(self).clone()
    }
    fn latest(&self) -> Option<Arc<MetricsTick>> {
        Engine::latest(self)
    }
    fn recent(&self, seconds: u32) -> Vec<HistoryPoint> {
        self.history(seconds)
    }
    fn set_benchmark_mode(&self, active: bool) {
        Engine::set_benchmark_mode(self, active);
    }
    fn power(&self) -> PowerState {
        crate::platform::power_state()
    }
}

/// Enables Benchmark Mode for its lifetime; dropping it (including on
/// error or cancellation paths) restores normal monitoring.
pub struct BenchmarkModeGuard<'a> {
    probe: &'a dyn SystemProbe,
}

impl<'a> BenchmarkModeGuard<'a> {
    pub fn new(probe: &'a dyn SystemProbe) -> Self {
        probe.set_benchmark_mode(true);
        Self { probe }
    }
}

impl Drop for BenchmarkModeGuard<'_> {
    fn drop(&mut self) {
        self.probe.set_benchmark_mode(false);
    }
}

/// CPU used by everything except this app in one tick, in percent.
pub fn external_cpu(tick: &MetricsTick, self_pid: u32) -> f32 {
    let own = tick
        .processes
        .iter()
        .find(|p| p.pid == self_pid)
        .map_or(0.0, |p| p.cpu_percent);
    (tick.cpu.usage_percent - own).clamp(0.0, 100.0)
}

pub fn snapshot(probe: &dyn SystemProbe) -> EnvironmentSnapshot {
    let info = probe.static_info();
    let latest = probe.latest();
    let recent = probe.recent(LOAD_WINDOW_SECS);
    let mut env = EnvironmentSnapshot {
        timestamp_ms: unix_millis(),
        os_name: info.os_name.clone(),
        os_version: info.os_version.clone(),
        kernel_version: info.kernel_version.clone(),
        architecture: info.architecture.clone(),
        cpu_brand: info.cpu_brand.clone(),
        cpu_vendor: info.cpu_vendor.clone(),
        physical_cores: info.physical_core_count,
        logical_cores: info.logical_core_count,
        total_memory_bytes: info.total_memory_bytes,
        app_version: info.app_version.clone(),
        power: probe.power(),
        ..EnvironmentSnapshot::default()
    };
    if let Some(tick) = latest {
        // Average total CPU over the window smooths single-tick spikes; this
        // app's own share comes from the latest process list.
        let total = if recent.is_empty() {
            tick.cpu.usage_percent
        } else {
            recent.iter().map(|p| p.cpu).sum::<f32>() / recent.len() as f32
        };
        let own = tick.cpu.usage_percent - external_cpu(&tick, info.self_pid);
        env.cpu_usage_percent = Some(total);
        env.external_cpu_percent = Some((total - own).clamp(0.0, 100.0));
        env.memory_used_percent = Some(tick.memory.used_percent);
        env.disk_active_percent = tick
            .disks
            .iter()
            .filter_map(|d| d.active_percent)
            .reduce(f64::max);
        env.network_rx_bytes_per_sec = Some(tick.point.net_rx);
        env.network_tx_bytes_per_sec = Some(tick.point.net_tx);
        env.cpu_frequency_mhz = tick.cpu.frequency_mhz;
    }
    env
}

pub fn load_level(env: &EnvironmentSnapshot) -> LoadLevel {
    match env.external_cpu_percent {
        Some(p) if p >= HIGH_LOAD_PERCENT => LoadLevel::High,
        Some(p) if p >= ELEVATED_LOAD_PERCENT => LoadLevel::Elevated,
        _ => LoadLevel::Idle,
    }
}

/// Warnings shown before a run starts.
pub fn preflight_warnings(env: &EnvironmentSnapshot) -> Vec<BenchmarkWarning> {
    let mut warnings = Vec::new();
    if load_level(env) != LoadLevel::Idle
        && let Some(p) = env.external_cpu_percent
    {
        warnings.push(BenchmarkWarning::new(
            WarningCode::HighBackgroundLoad,
            format!(
                "High background CPU activity detected. Current CPU usage by other \
                 applications: {p:.0}%. Other applications may affect benchmark accuracy."
            ),
        ));
    }
    if env.power.source == PowerSource::Battery {
        warnings.push(BenchmarkWarning::new(
            WarningCode::OnBattery,
            "This device is running on battery power. Power management may reduce \
             benchmark performance.",
        ));
    }
    warnings
}

pub fn fingerprint(env: &EnvironmentSnapshot) -> HardwareFingerprint {
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    HardwareFingerprint {
        cpu_brand: env.cpu_brand.trim().to_owned(),
        logical_cores: env.logical_cores,
        memory_gib: (env.total_memory_bytes as f64 / GIB).round() as u32,
        os: [env.os_name.as_deref(), env.os_version.as_deref()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" "),
        gpu: None,
        disk: None,
    }
}

/// Human-readable differences between two fingerprints. OS updates are
/// reported too, since they can legitimately change results.
pub fn fingerprint_changes(a: &HardwareFingerprint, b: &HardwareFingerprint) -> Vec<String> {
    let mut changes = Vec::new();
    let mut diff = |label: &str, x: String, y: String| {
        if x != y {
            changes.push(format!("{label}: {x} → {y}"));
        }
    };
    diff("CPU", a.cpu_brand.clone(), b.cpu_brand.clone());
    diff(
        "Logical processors",
        a.logical_cores.to_string(),
        b.logical_cores.to_string(),
    );
    diff(
        "Memory",
        format!("{} GiB", a.memory_gib),
        format!("{} GiB", b.memory_gib),
    );
    diff("Operating system", a.os.clone(), b.os.clone());
    let opt = |v: &Option<String>| v.clone().unwrap_or_else(|| "not tested".into());
    if a.gpu.is_some() && b.gpu.is_some() {
        diff("GPU", opt(&a.gpu), opt(&b.gpu));
    }
    if a.disk.is_some() && b.disk.is_some() {
        diff("Disk", opt(&a.disk), opt(&b.disk));
    }
    changes
}

/// Hardware changes that make a comparison not meaningful (OS changes are
/// informative only).
pub fn hardware_changed(a: &HardwareFingerprint, b: &HardwareFingerprint) -> bool {
    a.cpu_brand != b.cpu_brand
        || a.logical_cores != b.logical_cores
        || a.memory_gib != b.memory_gib
        || (a.gpu.is_some() && b.gpu.is_some() && a.gpu != b.gpu)
        || (a.disk.is_some() && b.disk.is_some() && a.disk != b.disk)
}

/// Samples other applications' CPU use from engine snapshots while a run is
/// active. It only reads already-collected ticks, so it adds no OS polling.
pub struct InterferenceMonitor {
    stop: Arc<AtomicBool>,
    samples: Arc<Mutex<Vec<f32>>>,
    handle: Option<JoinHandle<()>>,
}

impl InterferenceMonitor {
    pub fn start(probe: Arc<dyn SystemProbe>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let samples = Arc::new(Mutex::new(Vec::new()));
        let self_pid = probe.static_info().self_pid;
        let handle = {
            let stop = Arc::clone(&stop);
            let samples = Arc::clone(&samples);
            thread::Builder::new()
                .name("benchmark-interference".into())
                .spawn(move || {
                    let mut last_seq = 0;
                    while !stop.load(Ordering::SeqCst) {
                        if let Some(tick) = probe.latest()
                            && tick.seq != last_seq
                        {
                            last_seq = tick.seq;
                            samples
                                .lock()
                                .unwrap_or_else(PoisonError::into_inner)
                                .push(external_cpu(&tick, self_pid));
                        }
                        thread::sleep(Duration::from_millis(250));
                    }
                })
                .ok()
        };
        Self {
            stop,
            samples,
            handle,
        }
    }

    /// Starts a new measurement window (e.g. a new category).
    pub fn reset(&self) {
        self.samples
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }

    /// Report for the samples since the last reset.
    pub fn report(&self) -> Option<InterferenceReport> {
        let samples = self.samples.lock().unwrap_or_else(PoisonError::into_inner);
        if samples.is_empty() {
            return None;
        }
        let mean = samples.iter().map(|v| f64::from(*v)).sum::<f64>() / samples.len() as f64;
        let max = samples.iter().copied().fold(0.0f32, f32::max);
        Some(InterferenceReport {
            mean_external_cpu_percent: mean,
            max_external_cpu_percent: f64::from(max),
            samples: u32::try_from(samples.len()).unwrap_or(u32::MAX),
        })
    }
}

impl Drop for InterferenceMonitor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::models::{
        CpuSnapshot, EngineStats, MemorySnapshot, Platform, ProcessRow, ProcessState,
    };
    use std::sync::atomic::AtomicUsize;

    /// Deterministic probe for coordinator and environment tests.
    pub struct FakeProbe {
        pub cpu: Mutex<f32>,
        pub own_cpu: f32,
        pub power: PowerState,
        pub mode_changes: AtomicUsize,
        pub mode: AtomicBool,
    }

    impl FakeProbe {
        pub fn new(cpu: f32) -> Arc<Self> {
            Arc::new(Self {
                cpu: Mutex::new(cpu),
                own_cpu: 1.0,
                power: PowerState::default(),
                mode_changes: AtomicUsize::new(0),
                mode: AtomicBool::new(false),
            })
        }
    }

    pub const SELF_PID: u32 = 4242;

    impl SystemProbe for FakeProbe {
        fn static_info(&self) -> StaticSystemInfo {
            StaticSystemInfo {
                platform: Platform::current(),
                os_name: Some("TestOS".into()),
                os_version: Some("1".into()),
                long_os_version: None,
                kernel_version: Some("1.0".into()),
                hostname: Some("never-stored".into()),
                architecture: "x86_64".into(),
                cpu_brand: "Test CPU".into(),
                cpu_vendor: "Test".into(),
                physical_core_count: Some(4),
                logical_core_count: 8,
                total_memory_bytes: 16 * 1024 * 1024 * 1024,
                boot_time_secs: None,
                app_version: "0.1.0".into(),
                self_pid: SELF_PID,
            }
        }
        fn latest(&self) -> Option<Arc<MetricsTick>> {
            let cpu = *self.cpu.lock().unwrap();
            Some(Arc::new(tick(cpu, self.own_cpu)))
        }
        fn recent(&self, _seconds: u32) -> Vec<HistoryPoint> {
            Vec::new()
        }
        fn set_benchmark_mode(&self, active: bool) {
            self.mode.store(active, Ordering::SeqCst);
            self.mode_changes.fetch_add(1, Ordering::SeqCst);
        }
        fn power(&self) -> PowerState {
            self.power.clone()
        }
    }

    pub fn tick(cpu: f32, own_cpu: f32) -> MetricsTick {
        MetricsTick {
            seq: 1,
            timestamp_ms: 0,
            elapsed_ms: 1_000.0,
            cpu: CpuSnapshot {
                usage_percent: cpu,
                per_core: vec![cpu],
                frequency_mhz: Some(3_000),
                load_average: None,
            },
            memory: MemorySnapshot {
                total_bytes: 100,
                used_bytes: 50,
                available_bytes: 50,
                used_percent: 50.0,
                cached_bytes: None,
                buffers_bytes: None,
                swap_total_bytes: 0,
                swap_used_bytes: 0,
                commit_used_bytes: None,
                commit_limit_bytes: None,
            },
            disks: Vec::new(),
            networks: Vec::new(),
            processes: vec![ProcessRow {
                pid: SELF_PID,
                parent_pid: None,
                name: "taskmanager".into(),
                status: ProcessState::Running,
                cpu_percent: own_cpu,
                memory_bytes: 0,
                virtual_memory_bytes: 0,
                disk_read_per_sec: 0.0,
                disk_write_per_sec: 0.0,
                user: None,
                start_time: 0,
                threads: None,
                priority: None,
                exe: None,
                cmd: None,
                session_id: None,
                restricted: false,
            }],
            alerts: Vec::new(),
            point: HistoryPoint {
                t: 0,
                cpu,
                per_core: vec![cpu],
                mem_percent: 50.0,
                mem_used: 50,
                swap_used: 0,
                net_rx: 0.0,
                net_tx: 0.0,
                disk_read: 0.0,
                disk_write: 0.0,
            },
            engine: EngineStats {
                poll_duration_ms: 0.0,
                interval_ms: 1_000,
                process_count: 1,
                last_payload_bytes: 0,
                benchmark_active: false,
            },
        }
    }

    #[test]
    fn external_cpu_excludes_own_process() {
        assert!((external_cpu(&tick(60.0, 45.0), SELF_PID) - 15.0).abs() < 1e-6);
        assert!((external_cpu(&tick(10.0, 0.0), 1) - 10.0).abs() < 1e-6);
    }

    #[test]
    fn load_levels_and_warnings() {
        let probe = FakeProbe::new(5.0);
        let env = snapshot(&*probe);
        assert_eq!(load_level(&env), LoadLevel::Idle);
        assert!(preflight_warnings(&env).is_empty());
        assert!(env.os_name.is_some());

        *probe.cpu.lock().unwrap() = 38.0;
        let env = snapshot(&*probe);
        assert_eq!(load_level(&env), LoadLevel::High);
        let w = preflight_warnings(&env);
        assert_eq!(w[0].code, WarningCode::HighBackgroundLoad);
        assert!(w[0].message.contains("37%"));
    }

    #[test]
    fn battery_is_warned() {
        let probe = Arc::new(FakeProbe {
            power: PowerState {
                source: PowerSource::Battery,
                battery_percent: Some(50.0),
            },
            ..Arc::try_unwrap(FakeProbe::new(0.0)).ok().unwrap()
        });
        let w = preflight_warnings(&snapshot(&*probe));
        assert!(w.iter().any(|w| w.code == WarningCode::OnBattery));
    }

    #[test]
    fn guard_restores_benchmark_mode() {
        let probe = FakeProbe::new(0.0);
        {
            let _g = BenchmarkModeGuard::new(&*probe);
            assert!(probe.mode.load(Ordering::SeqCst));
        }
        assert!(!probe.mode.load(Ordering::SeqCst));
        assert_eq!(probe.mode_changes.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn fingerprint_detects_hardware_changes() {
        let probe = FakeProbe::new(0.0);
        let a = fingerprint(&snapshot(&*probe));
        assert_eq!(a.memory_gib, 16);
        let mut b = a.clone();
        assert!(!hardware_changed(&a, &b));
        b.os = "TestOS 2".into();
        assert!(!hardware_changed(&a, &b));
        assert_eq!(fingerprint_changes(&a, &b).len(), 1);
        b.gpu = Some("GPU B".into());
        assert!(
            !hardware_changed(&a, &b),
            "untested GPU on one side is not a change"
        );
        let mut c = b.clone();
        c.gpu = Some("GPU C".into());
        assert!(hardware_changed(&b, &c));
    }

    #[test]
    fn hostname_is_not_part_of_snapshot() {
        let probe = FakeProbe::new(0.0);
        let json = serde_json::to_string(&snapshot(&*probe)).unwrap();
        assert!(!json.contains("never-stored"));
    }
}
