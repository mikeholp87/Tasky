//! Benchmark data model, shared with the frontend through generated types.
//!
//! Raw measurements are the source of truth; scores are derived from them
//! (see `scoring`) and can be recomputed if the scoring is ever revised.

use serde::{Deserialize, Serialize};

use crate::models::{InterfaceKind, Platform};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum BenchmarkKind {
    Cpu,
    Gpu,
    Disk,
    Internet,
}

/// User-facing profiles. They change repetition counts and sizes only; the
/// operations measured never change between profiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum BenchmarkProfile {
    Quick,
    Standard,
    Extended,
}

/// One enum instead of `isRunning`/`isCancelled` flags, so impossible
/// combinations cannot be represented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum BenchmarkState {
    Idle,
    Preparing,
    Preflight,
    WarmingUp,
    Running,
    CoolingDown,
    Finalizing,
    Completed,
    Cancelled,
    Failed,
}

impl BenchmarkState {
    /// A run is in progress and owns the benchmark lock.
    pub fn is_active(self) -> bool {
        !matches!(
            self,
            Self::Idle | Self::Completed | Self::Cancelled | Self::Failed
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum Comparability {
    /// Same method id means the same workload on every platform.
    CrossPlatform,
    PlatformSpecific,
}

/// Identifies exactly what was measured. Results with different `method_id`
/// must not be compared.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct MethodInfo {
    pub method_id: String,
    pub score_version: String,
    pub comparability: Comparability,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum PowerSource {
    Ac,
    Battery,
    Unknown,
}

/// Facts about the machine and its state when the run started.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Environment {
    pub platform: Platform,
    pub distribution: Option<String>,
    pub kernel: Option<String>,
    pub architecture: String,
    pub cpu_model: String,
    pub physical_cores: Option<u32>,
    pub logical_cores: u32,
    pub total_memory_bytes: u64,
    pub power_source: PowerSource,
    /// Linux cpufreq governor, e.g. "powersave". Recorded, never changed.
    pub cpu_governor: Option<String>,
    /// "virtual machine" or a container name, when detected.
    pub virtualization: Option<String>,
    /// System-wide CPU use just before the run, percent.
    pub background_cpu_percent: Option<f32>,
    /// Built without optimisations: timings are not representative.
    pub development_build: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum CpuMode {
    SingleCore,
    MultiCore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum WorkloadId {
    Integer,
    FloatingPoint,
    Hash,
    Compress,
    Decompress,
}

/// All iterations of one workload in one mode. `samples` holds every raw
/// iteration's throughput; the reported value is the median.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct WorkloadResult {
    pub id: WorkloadId,
    pub mode: CpuMode,
    pub label: String,
    pub unit: String,
    pub workers: u32,
    pub samples: Vec<f64>,
    pub median: f64,
    pub mean: f64,
    pub min: f64,
    pub max: f64,
    pub std_dev: Option<f64>,
    pub cv_percent: Option<f64>,
    pub p95: Option<f64>,
    /// Indices into `samples` flagged by the IQR rule. Kept in the result.
    pub outlier_indices: Vec<u32>,
    /// Extra context, e.g. the compression ratio.
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SustainedWindow {
    pub t_secs: f64,
    pub mops: f64,
    pub frequency_mhz: Option<f64>,
    /// Only when the sensor can be identified as the CPU.
    pub temperature_c: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SustainedResult {
    pub windows: Vec<SustainedWindow>,
    pub peak_mops: f64,
    pub median_mops: f64,
    /// Median of the last third of the windows.
    pub sustained_mops: f64,
    /// Drop from the first third to the last third, percent (0 if none).
    pub decline_percent: f64,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct CpuScores {
    pub single_core: u32,
    pub multi_core: u32,
}

/// A rough read on how repeatable the run was. A heuristic from the spread
/// of iterations and the warnings; not a statistical confidence interval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum Stability {
    High,
    Medium,
    Low,
}

/// What kind of graphics device an adapter is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum GpuDeviceType {
    Discrete,
    Integrated,
    Virtual,
    /// A renderer that runs on the CPU (e.g. llvmpipe). Not a GPU result.
    Software,
    Other,
}

/// A graphics adapter the benchmark can run on, as reported by the driver.
/// Nothing is guessed: names and driver text are the driver's own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct GpuAdapterInfo {
    /// Stable within one enumeration; pass it back to select this adapter.
    pub id: String,
    pub name: String,
    pub vendor_id: u32,
    pub device_id: u32,
    pub device_type: GpuDeviceType,
    /// "vulkan", "dx12", ...
    pub backend: String,
    pub driver: String,
    pub driver_info: String,
    /// Can measure GPU execution time directly.
    pub supports_timestamps: bool,
    pub max_buffer_bytes: u64,
    pub max_storage_binding_bytes: u64,
    pub max_workgroups_per_dimension: u32,
    /// The adapter the benchmark would pick by default.
    pub recommended: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum GpuWorkloadId {
    Integer,
    FloatingPoint,
    Memory,
}

/// How GPU execution time was measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum TimingMethod {
    /// Timestamps written by the GPU around the dispatch.
    GpuTimestamps,
    /// Wall clock from submit until completion was confirmed; includes
    /// driver and queue overhead.
    WallClock,
}

/// All iterations of one GPU workload. `samples` are raw per-dispatch
/// throughputs; the reported value is the median.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct GpuWorkloadResult {
    pub id: GpuWorkloadId,
    pub label: String,
    pub unit: String,
    pub samples: Vec<f64>,
    pub median: f64,
    pub mean: f64,
    pub min: f64,
    pub max: f64,
    pub std_dev: Option<f64>,
    pub cv_percent: Option<f64>,
    pub p95: Option<f64>,
    pub outlier_indices: Vec<u32>,
    /// Operations (or bytes) in one dispatch; known, not estimated.
    pub work_per_dispatch: f64,
    /// Median time of one dispatch.
    pub dispatch_ms: f64,
    /// The output was checked against a CPU reference.
    pub validated: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct GpuResult {
    pub adapter: GpuAdapterInfo,
    pub timing_method: TimingMethod,
    pub workloads: Vec<GpuWorkloadResult>,
    /// Sustained phase (Extended); `mops` holds mega-operations per second.
    pub sustained: Option<SustainedResult>,
    /// Things worth knowing about this device, e.g. shared memory.
    pub notes: Vec<String>,
}

// ------------------------------------------------------------------ disk

/// How file I/O was done. Part of the method id: results measured with
/// different modes are different measurements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum IoMode {
    /// `O_DIRECT`: bypasses the page cache, so the device is measured.
    Direct,
    /// Ordinary file I/O through the page cache: results reflect the cache.
    Buffered,
}

/// A place the disk benchmark may create its temporary file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct DiskTargetInfo {
    /// Stable id the UI passes back; the UI never supplies a path.
    pub id: String,
    pub label: String,
    /// Folder the temporary file will be created in.
    pub directory: String,
    pub mount_point: String,
    pub file_system: String,
    /// Backing device, e.g. `/dev/nvme0n1p2`.
    pub device: Option<String>,
    pub free_bytes: u64,
    pub total_bytes: u64,
    pub recommended: bool,
    /// Cautions about this location (e.g. a container's overlay filesystem).
    pub notes: Vec<String>,
}

/// What the kernel says about the physical device, where it says anything.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct DiskDeviceInfo {
    pub name: String,
    pub model: Option<String>,
    pub rotational: Option<bool>,
    /// "nvme", "sata", "usb", ... when it can be told from the device path.
    pub transport: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum DiskPhaseKind {
    SequentialRead,
    SequentialWrite,
    RandomRead,
    RandomWrite,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct LatencyBucket {
    /// Exclusive upper bound of the bucket in microseconds.
    pub upper_us: f64,
    pub count: u64,
}

/// Per-operation latency. Percentiles are exact, computed from every
/// individual sample, never from averaged intervals; the histogram is the
/// condensed form kept in saved results.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct LatencySummary {
    pub samples: u64,
    pub mean_us: f64,
    pub p50_us: f64,
    pub p95_us: f64,
    pub p99_us: f64,
    pub max_us: f64,
    pub histogram: Vec<LatencyBucket>,
}

/// One phase of the disk test. `samples` are raw per-segment throughputs; the
/// reported value is their median.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct DiskPhaseResult {
    pub kind: DiskPhaseKind,
    pub label: String,
    pub block_bytes: u32,
    /// Threads doing I/O at the same time.
    pub concurrency: u32,
    /// `true` only with direct I/O, where each thread's operation really is
    /// outstanding at the device. With buffered I/O it is just concurrency.
    pub queue_depth_exact: bool,
    pub operations: u64,
    pub bytes: u64,
    pub seconds: f64,
    /// MB/s (decimal), one per segment.
    pub samples: Vec<f64>,
    pub median_mbps: f64,
    pub mean_mbps: f64,
    pub min_mbps: f64,
    pub max_mbps: f64,
    pub cv_percent: Option<f64>,
    pub outlier_indices: Vec<u32>,
    pub median_iops: f64,
    pub latency: Option<LatencySummary>,
    /// Data read back matched what was written.
    pub validated: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct DiskResult {
    pub target: DiskTargetInfo,
    pub device: Option<DiskDeviceInfo>,
    pub io_mode: IoMode,
    /// Why buffered I/O was used, when direct I/O was not possible.
    pub io_mode_note: Option<String>,
    pub file_bytes: u64,
    /// Actual bytes written to the file during the run (flash wear).
    pub bytes_written: u64,
    pub bytes_read: u64,
    pub phases: Vec<DiskPhaseResult>,
    pub notes: Vec<String>,
}

// -------------------------------------------------------------- internet

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct LatencyStats {
    pub samples: u32,
    pub min_ms: f64,
    pub median_ms: f64,
    pub p95_ms: f64,
    pub max_ms: f64,
    /// Mean absolute difference between consecutive round-trip times.
    pub jitter_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct TransferResult {
    /// Megabits per second (decimal), from bytes actually moved.
    pub median_mbps: f64,
    pub mean_mbps: f64,
    pub peak_mbps: f64,
    /// One per measurement window, after the ramp-up is excluded.
    pub samples: Vec<f64>,
    pub bytes: u64,
    pub seconds: f64,
    pub connections: u32,
    /// Latency measured while this transfer was saturating the link.
    pub loaded_latency: Option<LatencyStats>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct NetworkPath {
    /// Interface carrying the default route.
    pub interface: Option<String>,
    pub kind: Option<InterfaceKind>,
    /// A VPN or tunnel looks active; results may reflect it.
    pub vpn_suspected: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct InternetResult {
    pub provider: String,
    pub host: String,
    /// The server's data-centre code. Nothing about the user's own address or
    /// location is recorded.
    pub server_location: Option<String>,
    pub path: NetworkPath,
    pub idle_latency: Option<LatencyStats>,
    pub download: Option<TransferResult>,
    pub upload: Option<TransferResult>,
    pub bytes_downloaded: u64,
    pub bytes_uploaded: u64,
    /// Always `None`: HTTP cannot measure packet loss, and a failed request is
    /// not a lost packet.
    pub packet_loss_percent: Option<f64>,
    pub notes: Vec<String>,
}

/// What an internet run would do, shown to the user before they confirm.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct InternetPlan {
    pub provider: String,
    pub host: String,
    pub max_download_bytes: u64,
    pub max_upload_bytes: u64,
    pub estimated_seconds: u32,
}

/// What a disk run would do, shown to the user before they run it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct DiskPlan {
    pub target_id: String,
    pub directory: String,
    pub file_bytes: u64,
    /// Approximate total written to the drive.
    pub estimated_bytes_written: u64,
    pub estimated_seconds: u32,
    /// Why the run cannot happen here, e.g. not enough free space.
    pub blocked: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkResult {
    pub id: String,
    pub kind: BenchmarkKind,
    pub profile: BenchmarkProfile,
    pub started_at_ms: u64,
    pub duration_ms: u64,
    pub method: MethodInfo,
    pub environment: Environment,
    pub workloads: Vec<WorkloadResult>,
    pub sustained: Option<SustainedResult>,
    /// `None` for development builds, which are not scored.
    pub scores: Option<CpuScores>,
    /// GPU runs only. Defaults keep history saved before GPU support loadable.
    #[serde(default)]
    pub gpu: Option<GpuResult>,
    #[serde(default)]
    pub gpu_score: Option<u32>,
    /// Disk runs only.
    #[serde(default)]
    pub disk: Option<DiskResult>,
    /// `None` unless direct I/O was used: buffered results reflect the cache.
    #[serde(default)]
    pub disk_score: Option<u32>,
    /// Internet runs only. There is deliberately no internet score.
    #[serde(default)]
    pub internet: Option<InternetResult>,
    pub warnings: Vec<String>,
    pub stability: Stability,
    pub outlier_method: String,
    /// What happened with thread pinning, e.g. "not pinned".
    pub affinity: String,
}

/// A live reading shown while a benchmark runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct LiveReading {
    pub label: String,
    pub value: f64,
    pub unit: String,
}

/// Progress snapshot pushed to the UI (at most ~10 per second).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkStatus {
    pub state: BenchmarkState,
    pub run_id: Option<String>,
    pub kind: Option<BenchmarkKind>,
    pub profile: Option<BenchmarkProfile>,
    /// Human-readable current step, e.g. "Multi-core · Floating point".
    pub stage: String,
    /// 0..=1 across the whole run.
    pub fraction: f32,
    pub live: Option<LiveReading>,
    pub elapsed_ms: u64,
    /// Failure or cancellation explanation.
    pub message: Option<String>,
    pub cpu_percent: Option<f32>,
    pub frequency_mhz: Option<f64>,
    pub temperature_c: Option<f32>,
    pub development_build: bool,
}

impl BenchmarkStatus {
    pub fn idle() -> Self {
        Self {
            state: BenchmarkState::Idle,
            run_id: None,
            kind: None,
            profile: None,
            stage: String::new(),
            fraction: 0.0,
            live: None,
            elapsed_ms: 0,
            message: None,
            cpu_percent: None,
            frequency_mhz: None,
            temperature_c: None,
            development_build: cfg!(debug_assertions),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_in_flight_states_hold_the_lock() {
        use BenchmarkState::*;
        for s in [
            Preparing,
            Preflight,
            WarmingUp,
            Running,
            CoolingDown,
            Finalizing,
        ] {
            assert!(s.is_active(), "{s:?}");
        }
        for s in [Idle, Completed, Cancelled, Failed] {
            assert!(!s.is_active(), "{s:?}");
        }
    }
}
