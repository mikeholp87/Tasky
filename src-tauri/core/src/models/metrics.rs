use serde::Serialize;

use super::{AlertEvent, ProcessRow};

/// One sample of all dynamic metrics, emitted to the frontend on every tick.
///
/// Values a platform cannot report reliably are `None`; the UI shows
/// "Unavailable on this platform" rather than an invented number.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct MetricsTick {
    /// Monotonic sequence number, starting at 1.
    pub seq: u64,
    /// Unix time in milliseconds when the sample was taken.
    pub timestamp_ms: u64,
    /// Actual time since the previous sample, used for every rate.
    pub elapsed_ms: f64,
    pub cpu: CpuSnapshot,
    pub memory: MemorySnapshot,
    pub disks: Vec<DiskSnapshot>,
    pub networks: Vec<NetworkInterfaceSnapshot>,
    pub processes: Vec<ProcessRow>,
    /// Alerts that fired on this tick (usually empty).
    pub alerts: Vec<AlertEvent>,
    /// Compact summary of this tick, appended to history buffers.
    pub point: HistoryPoint,
    pub engine: EngineStats,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct CpuSnapshot {
    pub usage_percent: f32,
    pub per_core: Vec<f32>,
    /// Average reported frequency across logical processors, in MHz.
    pub frequency_mhz: Option<u64>,
    /// 1/5/15-minute load averages (Unix only).
    pub load_average: Option<LoadAverage>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct LoadAverage {
    pub one: f64,
    pub five: f64,
    pub fifteen: f64,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct MemorySnapshot {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub used_percent: f64,
    /// Linux: page cache. Windows: system file cache.
    pub cached_bytes: Option<u64>,
    /// Linux only: kernel buffers.
    pub buffers_bytes: Option<u64>,
    pub swap_total_bytes: u64,
    pub swap_used_bytes: u64,
    /// Windows commit charge (RAM + page file in use).
    pub commit_used_bytes: Option<u64>,
    pub commit_limit_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct DiskSnapshot {
    pub name: String,
    pub mount_point: String,
    pub file_system: String,
    pub kind: DiskKind,
    pub removable: bool,
    pub read_only: bool,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub used_bytes: u64,
    pub used_percent: f64,
    pub read_bytes_per_sec: Option<f64>,
    pub write_bytes_per_sec: Option<f64>,
    /// Completed read/write operations per second (IOPS).
    pub read_ops_per_sec: Option<f64>,
    pub write_ops_per_sec: Option<f64>,
    /// Share of time the disk was busy, like Task Manager's "Active time".
    pub active_percent: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum DiskKind {
    Ssd,
    Hdd,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct NetworkInterfaceSnapshot {
    pub name: String,
    pub rx_bytes_per_sec: f64,
    pub tx_bytes_per_sec: f64,
    pub total_rx_bytes: u64,
    pub total_tx_bytes: u64,
    pub ipv4: Vec<String>,
    pub ipv6: Vec<String>,
    pub mac_address: Option<String>,
    /// Heuristic "primary" interface: has an address and the most traffic.
    pub is_primary: bool,
}

/// Compact per-tick summary kept in history ring buffers.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct HistoryPoint {
    /// Unix milliseconds.
    pub t: u64,
    pub cpu: f32,
    pub per_core: Vec<f32>,
    pub mem_percent: f32,
    pub mem_used: u64,
    pub swap_used: u64,
    pub net_rx: f64,
    pub net_tx: f64,
    pub disk_read: f64,
    pub disk_write: f64,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct EngineStats {
    /// Wall time spent collecting this sample.
    pub poll_duration_ms: f64,
    pub interval_ms: u32,
    pub process_count: u32,
    /// Serialized size of the previous tick, to keep IPC cost visible.
    pub last_payload_bytes: u32,
    /// A benchmark is running: polling is slowed and part of the load is the
    /// benchmark itself.
    pub benchmark_active: bool,
}
