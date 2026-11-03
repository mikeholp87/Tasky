//! Benchmark data model, shared with the frontend through generated types.
//!
//! Raw measurements are the source of truth; scores are derived from them
//! (see `scoring`) and can be recomputed if the scoring is ever revised.

use serde::{Deserialize, Serialize};

use crate::models::Platform;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum BenchmarkKind {
    Cpu,
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
