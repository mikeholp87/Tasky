//! Owns the sysinfo handles and turns one OS sample into a [`MetricsTick`].
//! Runs only on the engine thread, so it needs no locking.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use sysinfo::{CpuRefreshKind, System};

use crate::errors::{AppError, AppResult};
use crate::models::{EngineStats, HistoryPoint, MetricsTick, Platform, StaticSystemInfo};
use crate::monitoring::{
    cpu, disks::DiskTracker, memory, network::NetworkTracker, processes::ProcessTracker,
};
use crate::platform;

pub struct Collector {
    sys: System,
    disks: DiskTracker,
    networks: NetworkTracker,
    processes: ProcessTracker,
    last_sample: Instant,
    seq: u64,
}

impl Collector {
    pub fn new() -> AppResult<Self> {
        let mut sys = System::new()
            .map_err(|e| AppError::SystemQueryFailed(format!("cannot initialise: {e}")))?;
        // Prime counters so the first real sample has a baseline for CPU usage.
        sys.refresh_cpu_list(CpuRefreshKind::everything());
        sys.refresh_memory_specifics(memory::refresh_kind());
        Ok(Self {
            sys,
            disks: DiskTracker::new(),
            networks: NetworkTracker::new(),
            processes: ProcessTracker::new(),
            last_sample: Instant::now(),
            seq: 0,
        })
    }

    /// Static information, read once at startup.
    pub fn static_info(&self) -> StaticSystemInfo {
        let cpus = self.sys.cpus();
        let first = cpus.first();
        StaticSystemInfo {
            platform: Platform::current(),
            os_name: System::name().ok(),
            os_version: System::os_version().ok(),
            long_os_version: System::long_os_version().ok(),
            kernel_version: System::kernel_version().ok(),
            hostname: System::host_name().ok(),
            architecture: System::cpu_arch(),
            cpu_brand: first
                .map(|c| c.brand().trim().to_owned())
                .unwrap_or_default(),
            cpu_vendor: first.map(|c| c.vendor_id().to_owned()).unwrap_or_default(),
            physical_core_count: System::physical_core_count()
                .ok()
                .and_then(|n| u32::try_from(n).ok()),
            logical_core_count: u32::try_from(cpus.len()).unwrap_or(u32::MAX),
            total_memory_bytes: self.sys.total_memory(),
            boot_time_secs: System::boot_time().ok(),
            app_version: env!("CARGO_PKG_VERSION").to_owned(),
            self_pid: std::process::id(),
        }
    }

    pub fn sample(&mut self, interval_ms: u32, last_payload_bytes: u32) -> MetricsTick {
        let started = Instant::now();
        let elapsed = started.duration_since(self.last_sample);
        self.last_sample = started;
        self.seq += 1;

        self.sys.refresh_cpu_specifics(cpu::refresh_kind());
        self.sys.refresh_memory_specifics(memory::refresh_kind());

        let cpu = cpu::snapshot(&self.sys);
        let memory = memory::snapshot(&self.sys, platform::memory_extras());
        let disks = self.disks.sample(started, elapsed);
        let networks = self.networks.sample(elapsed);
        let processes = self.processes.sample(&mut self.sys, elapsed);
        let process_count = u32::try_from(processes.len()).unwrap_or(u32::MAX);

        let point = HistoryPoint {
            t: unix_millis(),
            cpu: cpu.usage_percent,
            per_core: cpu.per_core.clone(),
            mem_percent: memory.used_percent as f32,
            mem_used: memory.used_bytes,
            swap_used: memory.swap_used_bytes,
            net_rx: networks.iter().map(|n| n.rx_bytes_per_sec).sum(),
            net_tx: networks.iter().map(|n| n.tx_bytes_per_sec).sum(),
            disk_read: disks.iter().filter_map(|d| d.read_bytes_per_sec).sum(),
            disk_write: disks.iter().filter_map(|d| d.write_bytes_per_sec).sum(),
        };

        MetricsTick {
            seq: self.seq,
            timestamp_ms: point.t,
            elapsed_ms: duration_ms(elapsed),
            cpu,
            memory,
            disks,
            networks,
            processes,
            point,
            engine: EngineStats {
                poll_duration_ms: duration_ms(started.elapsed()),
                interval_ms,
                process_count,
                last_payload_bytes,
            },
        }
    }
}

fn duration_ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1_000.0
}

pub fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}
