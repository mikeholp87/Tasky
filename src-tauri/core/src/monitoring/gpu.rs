//! GPU metrics collection for Linux (NVIDIA via `nvidia-smi`, AMD/Intel via `/sys/class/drm`).

use std::path::Path;

use crate::models::GpuSnapshot;

/// Collects per-GPU snapshots. Returns an empty vec when no GPU is detected.
pub fn collect() -> Vec<GpuSnapshot> {
    // Try NVIDIA first (most common on desktop).
    if let Some(gpus) = nvidia_smi()
        && !gpus.is_empty()
    {
        return gpus;
    }

    // Fall back to /sys/class/drm for AMD/Intel.
    sysfs_drm_gpus()
}

/// Parse `nvidia-smi` CSV output.
fn nvidia_smi() -> Option<Vec<GpuSnapshot>> {
    let out = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,utilization.gpu,memory.total,memory.used,temperature.gpu,power.draw,power.limit,clocks.current.graphics,clocks.current.memory,fan.speed,pstate,pcie.link.gen.current,pcie.link.width.current,utilization.encoder,utilization.decoder,driver_version",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;

    if !out.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&out.stdout);
    Some(
        text.lines()
            .filter_map(|line| parse_nvidia_line(line.trim()))
            .collect(),
    )
}

fn parse_nvidia_line(line: &str) -> Option<GpuSnapshot> {
    let parts: Vec<&str> = line.split(',').map(str::trim).collect();
    if parts.len() < 4 {
        return None;
    }
    Some(GpuSnapshot {
        name: parts[0].to_owned(),
        usage_percent: parse_percent(parts[1]),
        total_vram_bytes: parse_u64_mb(parts[2])?,
        used_vram_bytes: parse_u64_mb(parts[3])?,
        temperature_celsius: parts.get(4).and_then(|s| s.parse::<f32>().ok()),
        power_watts: parts.get(5).and_then(|s| s.parse::<f32>().ok()),
        power_limit_watts: parts.get(6).and_then(|s| s.parse::<f32>().ok()),
        clock_mhz: parts.get(7).and_then(|s| s.parse::<u64>().ok()),
        memory_clock_mhz: parts.get(8).and_then(|s| s.parse::<u64>().ok()),
        fan_speed_percent: parts.get(9).and_then(|s| parse_percent(s)),
        pstate: clean_opt_str(parts.get(10).copied()),
        pcie_gen: parts.get(11).and_then(|s| s.parse::<u32>().ok()),
        pcie_width: parts.get(12).and_then(|s| s.parse::<u32>().ok()),
        encoder_usage_percent: parts.get(13).and_then(|s| parse_percent(s)),
        decoder_usage_percent: parts.get(14).and_then(|s| parse_percent(s)),
        driver_version: clean_opt_str(parts.get(15).copied()),
    })
}

fn clean_opt_str(s: Option<&str>) -> Option<String> {
    s.map(str::trim)
        .filter(|s| !s.is_empty() && *s != "[N/A]" && *s != "N/A")
        .map(ToOwned::to_owned)
}

/// Parse `/sys/class/drm/card*/` for AMD/Intel GPUs.
fn sysfs_drm_gpus() -> Vec<GpuSnapshot> {
    let mut gpus = Vec::new();
    let drm_base = Path::new("/sys/class/drm");

    // Find card directories (card0, card1, …) excluding connectors like card0-DP-1.
    if let Ok(entries) = std::fs::read_dir(drm_base) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            let file_name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let is_card = file_name
                .strip_prefix("card")
                .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()));
            if !path.is_dir() || !is_card || !path.join("device").exists() {
                continue;
            }

            let name = pci_device_name(&path);
            let usage = read_u8(&path.join("device").join("gpu_busy_percent"))
                .or_else(|| read_u8(&path.join("gpu_busy_percent")));
            let total = read_u64(&path, "device", "mem_info_vram_total");
            let used = read_u64(&path, "device", "mem_info_vram_used");

            gpus.push(GpuSnapshot {
                name: if name.is_empty() {
                    format!("GPU {file_name}")
                } else {
                    name
                },
                usage_percent: usage.map(f32::from),
                total_vram_bytes: total.unwrap_or(0),
                used_vram_bytes: used.unwrap_or(0),
                temperature_celsius: None,
                power_watts: None,
                power_limit_watts: None,
                clock_mhz: None,
                memory_clock_mhz: None,
                fan_speed_percent: None,
                pstate: None,
                pcie_gen: None,
                pcie_width: None,
                encoder_usage_percent: None,
                decoder_usage_percent: None,
                driver_version: None,
            });
        }
    }

    gpus
}

fn pci_device_name(card_path: &Path) -> String {
    let device = card_path.join("device");
    if let Ok(vendor) = std::fs::read_to_string(device.join("vendor")) {
        let vendor_id = vendor.trim();
        if let Ok(dev_id) = std::fs::read_to_string(device.join("device")) {
            let dev_id = dev_id.trim();
            match vendor_id {
                "0x10de" => return format!("NVIDIA GPU ({dev_id})"),
                "0x1002" => return format!("AMD GPU ({dev_id})"),
                "0x8086" => return format!("Intel GPU ({dev_id})"),
                _ => {}
            }
        }
    }
    String::new()
}

fn read_u8(path: &Path) -> Option<u8> {
    let content = std::fs::read_to_string(path).ok()?;
    content.trim().parse::<u8>().ok()
}

fn read_u64(card_path: &Path, sub: &str, file: &str) -> Option<u64> {
    let p = card_path.join(sub).join(file);
    let content = std::fs::read_to_string(p).ok()?;
    content.trim().parse::<u64>().ok()
}

fn parse_percent(s: &str) -> Option<f32> {
    s.parse::<f32>().ok().map(|v| v.clamp(0.0, 100.0))
}

fn parse_u64_mb(s: &str) -> Option<u64> {
    // nvidia-smi reports in MiB; convert to bytes.
    s.parse::<u64>().ok().map(|v| v * 1_024 * 1_024)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_nvidia_line_valid() {
        let gpu = parse_nvidia_line(
            "NVIDIA GeForce RTX 4090, 75.0, 24576, 8192, 62, 215.4, 450.0, 2520, 10501, 45, P0, 4, 16, 12, 5, 595.99.02",
        )
        .unwrap();
        assert_eq!(gpu.name, "NVIDIA GeForce RTX 4090");
        assert_eq!(gpu.usage_percent, Some(75.0));
        assert_eq!(gpu.total_vram_bytes, 24576 * 1_024 * 1_024);
        assert_eq!(gpu.used_vram_bytes, 8192 * 1_024 * 1_024);
        assert_eq!(gpu.temperature_celsius, Some(62.0));
        assert_eq!(gpu.power_watts, Some(215.4));
        assert_eq!(gpu.power_limit_watts, Some(450.0));
        assert_eq!(gpu.clock_mhz, Some(2520));
        assert_eq!(gpu.memory_clock_mhz, Some(10501));
        assert_eq!(gpu.fan_speed_percent, Some(45.0));
        assert_eq!(gpu.pstate.as_deref(), Some("P0"));
        assert_eq!(gpu.pcie_gen, Some(4));
        assert_eq!(gpu.pcie_width, Some(16));
        assert_eq!(gpu.encoder_usage_percent, Some(12.0));
        assert_eq!(gpu.decoder_usage_percent, Some(5.0));
        assert_eq!(gpu.driver_version.as_deref(), Some("595.99.02"));
    }

    #[test]
    fn parse_nvidia_line_invalid() {
        assert!(parse_nvidia_line("bad line").is_none());
        assert!(parse_nvidia_line("").is_none());
    }
}
