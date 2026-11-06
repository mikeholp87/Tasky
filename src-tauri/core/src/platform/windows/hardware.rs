//! On-demand hardware facts from local firmware registry data and Win32 APIs.
//! No shell commands, serial numbers, UUIDs, or elevation are needed.

use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::Registry::HKEY_LOCAL_MACHINE;
use windows::Win32::System::SystemInformation::{
    CacheData as CACHE_DATA, CacheInstruction as CACHE_INSTRUCTION, CacheUnified as CACHE_UNIFIED,
    GetLogicalProcessorInformation, RelationCache, SYSTEM_LOGICAL_PROCESSOR_INFORMATION,
};

use super::registry;
use crate::models::{BatteryInfo, CacheInfo, CpuDetails, HardwareInfo, MachineInfo};

fn machine() -> MachineInfo {
    let read = |name| {
        registry::string(
            HKEY_LOCAL_MACHINE,
            r"HARDWARE\DESCRIPTION\System\BIOS",
            name,
        )
    };
    MachineInfo {
        vendor: read("SystemManufacturer"),
        model: read("SystemProductName"),
        board: match (read("BaseBoardManufacturer"), read("BaseBoardProduct")) {
            (Some(v), Some(n)) => Some(format!("{v} {n}")),
            (v, n) => v.or(n),
        },
        bios_vendor: read("BIOSVendor"),
        bios_version: read("BIOSVersion"),
        bios_date: read("BIOSReleaseDate"),
        chassis: None,
    }
}

fn caches() -> Vec<CacheInfo> {
    let mut bytes = 0u32;
    // SAFETY: query the required buffer size, without providing a buffer.
    let _ = unsafe { GetLogicalProcessorInformation(None, &mut bytes) };
    if bytes == 0 || bytes > 1024 * 1024 {
        return Vec::new();
    }
    for _ in 0..3 {
        let size = size_of::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION>();
        let mut entries =
            vec![SYSTEM_LOGICAL_PROCESSOR_INFORMATION::default(); (bytes as usize).div_ceil(size)];
        let capacity = (entries.len() * size) as u32;
        bytes = capacity;
        // SAFETY: typed, correctly aligned entries hold at least `bytes` bytes.
        if unsafe { GetLogicalProcessorInformation(Some(entries.as_mut_ptr()), &mut bytes) }
            .is_err()
        {
            if bytes > capacity && bytes <= 1024 * 1024 {
                continue;
            }
            return Vec::new();
        }
        entries.truncate(bytes as usize / size);
        let mut out = Vec::new();
        for entry in entries
            .iter()
            .filter(|e| e.Relationship == RelationCache && e.ProcessorMask & 1 != 0)
        {
            // SAFETY: the union arm matches the RelationCache discriminator.
            let cache = unsafe { entry.Anonymous.Cache };
            let kind = match cache.Type {
                CACHE_DATA => "Data",
                CACHE_INSTRUCTION => "Instruction",
                CACHE_UNIFIED => "Unified",
                _ => continue,
            };
            out.push(CacheInfo {
                level: cache.Level,
                kind: kind.into(),
                size_bytes: u64::from(cache.Size),
            });
        }
        out.sort_by(|a, b| a.level.cmp(&b.level).then(a.kind.cmp(&b.kind)));
        return out;
    }
    Vec::new()
}

#[cfg(target_arch = "x86_64")]
fn cpu() -> CpuDetails {
    use std::arch::x86_64::{__cpuid, CpuidResult};
    // Older supported compilers require an unsafe block for this intrinsic.
    #[allow(unused_unsafe)]
    fn cpuid(leaf: u32) -> CpuidResult {
        // SAFETY: CPUID is present on every x86-64 CPU; callers check optional leaves.
        unsafe { __cpuid(leaf) }
    }
    // CPUID is available on every x86-64 CPU. Optional leaves are checked.
    let (max, features, extended) = (cpuid(0).eax, cpuid(1), cpuid(0x8000_0000).eax);
    let intel_vtx = features.ecx & (1 << 5) != 0;
    // The extended leaf is called only if the CPU reports it exists.
    let amd_v = extended >= 0x8000_0001 && cpuid(0x8000_0001).ecx & (1 << 2) != 0;
    // Leaf 0x16 exists when max reports it.
    let max_mhz = (max >= 0x16)
        .then(|| cpuid(0x16).ebx & 0xffff)
        .filter(|&v| v > 0);
    CpuDetails {
        max_mhz,
        min_mhz: None,
        governor: None,
        caches: caches(),
        virtualization: if intel_vtx {
            Some("Intel VT-x".into())
        } else if amd_v {
            Some("AMD-V".into())
        } else {
            None
        },
        virtualized: features.ecx & (1 << 31) != 0,
    }
}

#[cfg(not(target_arch = "x86_64"))]
fn cpu() -> CpuDetails {
    CpuDetails {
        caches: caches(),
        ..Default::default()
    }
}

pub fn hardware_info() -> HardwareInfo {
    let mut power = SYSTEM_POWER_STATUS::default();
    // SAFETY: `power` is a valid writable SYSTEM_POWER_STATUS.
    let known = unsafe { GetSystemPowerStatus(&mut power) }.is_ok();
    let state = super::power_state();
    let batteries = if known && power.BatteryFlag != 255 && power.BatteryFlag & 128 == 0 {
        vec![BatteryInfo {
            name: "System battery".into(),
            percent: (power.BatteryLifePercent <= 100).then_some(power.BatteryLifePercent),
            status: Some(
                if power.BatteryFlag & 8 != 0 {
                    "Charging"
                } else if power.ACLineStatus == 0 {
                    "Discharging"
                } else {
                    "Connected to AC"
                }
                .into(),
            ),
            technology: None,
            cycle_count: None,
            health_percent: None,
        }]
    } else {
        Vec::new()
    };
    HardwareInfo {
        machine: machine(),
        cpu: cpu(),
        batteries,
        power_source: Some(state.source),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_local_hardware_without_elevation() {
        let info = hardware_info();
        assert!(
            info.cpu
                .caches
                .iter()
                .all(|c| c.level > 0 && c.size_bytes > 0)
        );
        assert!(
            info.batteries
                .iter()
                .all(|b| b.percent.is_none_or(|p| p <= 100))
        );
        assert!(info.power_source.is_some());
    }
}
