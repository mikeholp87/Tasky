//! Hardware facts from sysfs and procfs. Everything is best-effort: a missing
//! or unreadable file leaves its field empty. Serial numbers and UUIDs are
//! not read.

use std::fs;
use std::path::Path;

use crate::models::{BatteryInfo, CacheInfo, CpuDetails, HardwareInfo, MachineInfo};
use crate::platform::linux_facts as facts;

fn read(path: impl AsRef<Path>) -> Option<String> {
    Some(fs::read_to_string(path).ok()?.trim().to_owned())
}

fn dmi(name: &str) -> Option<String> {
    facts::clean_dmi(&read(Path::new("/sys/class/dmi/id").join(name))?)
}

fn machine() -> MachineInfo {
    MachineInfo {
        vendor: dmi("sys_vendor"),
        model: dmi("product_name").or_else(|| dmi("product_version")),
        board: match (dmi("board_vendor"), dmi("board_name")) {
            (Some(v), Some(n)) => Some(format!("{v} {n}")),
            (v, n) => v.or(n),
        },
        bios_vendor: dmi("bios_vendor"),
        bios_version: dmi("bios_version"),
        bios_date: dmi("bios_date"),
        chassis: read("/sys/class/dmi/id/chassis_type")
            .and_then(|c| facts::chassis_name(&c))
            .map(str::to_owned),
    }
}

fn mhz(path: &str) -> Option<u32> {
    // cpufreq reports kHz.
    Some(read(path)?.parse::<u32>().ok()? / 1000)
}

fn caches() -> Vec<CacheInfo> {
    let mut out = Vec::new();
    for index in 0..8 {
        let dir = format!("/sys/devices/system/cpu/cpu0/cache/index{index}");
        let (Some(level), Some(kind), Some(size)) = (
            read(format!("{dir}/level")).and_then(|v| v.parse::<u8>().ok()),
            read(format!("{dir}/type")),
            read(format!("{dir}/size")).and_then(|v| facts::parse_cache_size(&v)),
        ) else {
            continue;
        };
        out.push(CacheInfo {
            level,
            kind,
            size_bytes: size,
        });
    }
    out.sort_by_key(|c| (c.level, c.kind.clone()));
    out
}

fn cpu() -> CpuDetails {
    CpuDetails {
        max_mhz: mhz("/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq"),
        min_mhz: mhz("/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_min_freq"),
        governor: super::cpu_governor(),
        caches: caches(),
        virtualization: read("/proc/cpuinfo")
            .and_then(|t| facts::cpu_virtualization(&t))
            .map(str::to_owned),
        virtualized: super::is_virtualized(),
    }
}

fn batteries() -> Vec<BatteryInfo> {
    let Ok(entries) = fs::read_dir("/sys/class/power_supply") else {
        return Vec::new();
    };
    let mut out: Vec<BatteryInfo> = entries
        .flatten()
        .filter(|e| read(e.path().join("type")).as_deref() == Some("Battery"))
        // Peripherals (mice, headsets) also appear as batteries with scope "Device".
        .filter(|e| read(e.path().join("scope")).as_deref() != Some("Device"))
        .map(|e| {
            let dir = e.path();
            let num = |f: &str| read(dir.join(f)).and_then(|v| v.parse::<u64>().ok());
            // Batteries report energy (µWh) or charge (µAh), never both.
            let full = num("energy_full").or_else(|| num("charge_full"));
            let design = num("energy_full_design").or_else(|| num("charge_full_design"));
            BatteryInfo {
                name: e.file_name().to_string_lossy().into_owned(),
                percent: num("capacity").map(|c| c.min(100) as u8),
                status: read(dir.join("status")),
                technology: read(dir.join("technology")).filter(|t| t != "Unknown"),
                cycle_count: num("cycle_count")
                    .filter(|&c| c > 0)
                    .and_then(|c| u32::try_from(c).ok()),
                health_percent: full
                    .zip(design)
                    .and_then(|(f, d)| facts::battery_health(f, d)),
            }
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

pub fn hardware_info() -> HardwareInfo {
    let batteries = batteries();
    HardwareInfo {
        machine: machine(),
        cpu: cpu(),
        power_source: Some(super::power_source()),
        batteries,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs on any Linux machine: it must not panic and caches must be ordered.
    #[test]
    fn reads_without_panicking() {
        let info = hardware_info();
        let levels: Vec<u8> = info.cpu.caches.iter().map(|c| c.level).collect();
        assert!(levels.windows(2).all(|w| w[0] <= w[1]));
        assert!(
            info.batteries
                .iter()
                .all(|b| b.percent.is_none_or(|p| p <= 100))
        );
    }
}
