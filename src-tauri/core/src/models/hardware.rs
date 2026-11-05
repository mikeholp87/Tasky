use serde::Serialize;

use crate::benchmarks::PowerSource;

/// Hardware facts that need more than sysinfo reports. Fetched on demand by
/// the System Information page. Every field is `None`/empty where the
/// platform or the current account cannot read it; serial numbers and UUIDs
/// are deliberately never collected.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct HardwareInfo {
    pub machine: MachineInfo,
    pub cpu: CpuDetails,
    pub batteries: Vec<BatteryInfo>,
    pub power_source: Option<PowerSource>,
}

/// Firmware-provided identity (Linux: `/sys/class/dmi/id`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct MachineInfo {
    pub vendor: Option<String>,
    pub model: Option<String>,
    pub board: Option<String>,
    pub bios_vendor: Option<String>,
    pub bios_version: Option<String>,
    pub bios_date: Option<String>,
    /// e.g. "Laptop", "Desktop".
    pub chassis: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct CpuDetails {
    pub max_mhz: Option<u32>,
    pub min_mhz: Option<u32>,
    pub governor: Option<String>,
    /// Caches of one core; L3 is shared between cores.
    pub caches: Vec<CacheInfo>,
    /// "Intel VT-x" or "AMD-V" when the CPU offers hardware virtualization.
    pub virtualization: Option<String>,
    /// The system itself runs under a hypervisor.
    pub virtualized: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct CacheInfo {
    pub level: u8,
    /// "Data", "Instruction" or "Unified".
    pub kind: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct BatteryInfo {
    pub name: String,
    pub percent: Option<u8>,
    /// "Charging", "Discharging", "Full", ...
    pub status: Option<String>,
    pub technology: Option<String>,
    pub cycle_count: Option<u32>,
    /// Full-charge capacity as a share of the design capacity.
    pub health_percent: Option<u8>,
}
