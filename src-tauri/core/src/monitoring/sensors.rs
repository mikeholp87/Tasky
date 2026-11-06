//! Temperatures and GPUs through sysinfo. Both change slowly and some
//! sources are comparatively costly (hwmon reads can wake devices, GPU
//! queries go through the driver), so they refresh every few seconds and
//! the last values are repeated on the ticks in between.
//!
//! On Linux sysinfo reads `/sys/class/hwmon` and `/sys/class/drm` (names
//! from the system `pci.ids`), with NVML or Vulkan loaded at runtime only if
//! present. Nothing here is required: missing sources yield empty lists.

use std::time::{Duration, Instant};

use sysinfo::{Components, Gpus};

use crate::models::{GpuSnapshot, SensorReading};

const REFRESH: Duration = Duration::from_secs(2);
/// Drive sensors: reading an NVMe/SATA temperature sends a command to the
/// drive (measured 8 ms) and can keep it out of its deepest idle state.
const DRIVE_REFRESH: Duration = Duration::from_secs(30);

pub struct SensorTracker {
    components: Option<Components>,
    gpus: Option<Gpus>,
    last_refresh: Option<Instant>,
    last_drive_refresh: Option<Instant>,
    temperatures: Vec<SensorReading>,
    gpu_snapshots: Vec<GpuSnapshot>,
}

impl SensorTracker {
    pub fn new() -> Self {
        Self {
            components: Components::new_with_refreshed_list()
                .inspect_err(|e| tracing::debug!("temperature sensors unavailable: {e}"))
                .ok(),
            gpus: Gpus::new_with_refreshed_list()
                .inspect_err(|e| tracing::debug!("GPU enumeration unavailable: {e}"))
                .ok(),
            last_refresh: None,
            last_drive_refresh: None,
            temperatures: Vec::new(),
            gpu_snapshots: Vec::new(),
        }
    }

    /// Latest readings, refreshed when `REFRESH` has passed.
    pub fn sample(&mut self, now: Instant) -> (Vec<GpuSnapshot>, Vec<SensorReading>) {
        let due = self
            .last_refresh
            .is_none_or(|t| now.duration_since(t) >= REFRESH);
        if due {
            // The first sample uses the lists already refreshed in `new`.
            let first = self.last_refresh.is_none();
            self.last_refresh = Some(now);
            let drives_due = self
                .last_drive_refresh
                .is_none_or(|t| now.duration_since(t) >= DRIVE_REFRESH);
            if drives_due {
                self.last_drive_refresh = Some(now);
            }
            if let Some(c) = self.components.as_mut() {
                if !first {
                    // Re-reads only each sensor's current value. The list
                    // refresh would also re-scan every hwmon directory and
                    // re-read labels and thresholds (measured 11-19 ms).
                    for component in c.iter_mut() {
                        if drives_due || !is_drive(component.label()) {
                            component.refresh();
                        }
                    }
                }
                self.temperatures = temperatures(c);
            }
            if let Some(g) = self.gpus.as_mut() {
                if !first {
                    g.refresh(false);
                }
                self.gpu_snapshots = g.iter().map(gpu_snapshot).collect();
            }
        }
        (self.gpu_snapshots.clone(), self.temperatures.clone())
    }
}

/// hwmon chip names of storage drivers; sysinfo labels start with the chip.
fn is_drive(label: &str) -> bool {
    label.starts_with("nvme") || label.starts_with("drivetemp")
}

fn temperatures(components: &Components) -> Vec<SensorReading> {
    let mut out: Vec<SensorReading> = components
        .iter()
        .filter_map(|c| reading(c.label(), c.temperature()?, c.max(), c.critical()))
        .collect();
    out.sort_by(|a, b| a.label.cmp(&b.label));
    out
}

/// Rejects readings that are not physical temperatures: drivers report
/// unplugged probes as 0, -273 or huge sentinel values. Thresholds of 0
/// mean "not set".
fn reading(
    label: &str,
    celsius: f32,
    max: Option<f32>,
    critical: Option<f32>,
) -> Option<SensorReading> {
    let plausible = |v: f32| v.is_finite() && v > 0.0 && v < 200.0;
    plausible(celsius).then(|| SensorReading {
        label: label.trim().to_owned(),
        celsius,
        max_celsius: max.filter(|v| plausible(*v)),
        critical_celsius: critical.filter(|v| plausible(*v)),
    })
}

fn gpu_snapshot(g: &sysinfo::Gpu) -> GpuSnapshot {
    let vendor = g.vendor().map(str::to_owned);
    // sysinfo reads device-wide memory counters only for AMD (sysfs) and
    // NVIDIA (NVML). For other GPUs it falls back to Vulkan's memory budget,
    // whose "usage" is this process's own allocations and whose heap on an
    // integrated GPU is shared system RAM, so neither is shown.
    // ponytail: vendor-name check; move to a source flag if sysinfo exposes one.
    let global_memory = cfg!(target_os = "linux")
        && vendor
            .as_deref()
            .is_some_and(|v| v.contains("AMD") || v.contains("NVIDIA"));
    GpuSnapshot {
        id: g.pci().to_string(),
        // No model in pci.ids or from the driver: say what we know rather
        // than guess a marketing name.
        name: g
            .model()
            .map(str::to_owned)
            .or_else(|| vendor.as_ref().map(|v| format!("{v} GPU")))
            .unwrap_or_else(|| "GPU".into()),
        vendor,
        usage_percent: g
            .usage()
            .filter(|u| u.is_finite())
            .map(|u| u.clamp(0.0, 100.0)),
        memory_used_bytes: g.used_memory().filter(|_| global_memory),
        memory_total_bytes: g.total_memory().filter(|&t| t > 0 && global_memory),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_drive_sensors() {
        assert!(is_drive("nvme Composite WD_BLACK SN850X 2000GB"));
        assert!(is_drive("drivetemp temp1"));
        assert!(!is_drive("coretemp Core 0"));
    }

    #[test]
    fn filters_implausible_temperatures() {
        assert!(reading("x", 0.0, None, None).is_none());
        assert!(reading("x", -273.0, None, None).is_none());
        assert!(reading("x", 255.5, None, None).is_none());
        assert!(reading("x", f32::NAN, None, None).is_none());
        let r = reading(" coretemp Core 0 ", 48.0, Some(0.0), Some(100.0)).unwrap();
        assert_eq!(r.label, "coretemp Core 0");
        assert_eq!(r.max_celsius, None);
        assert_eq!(r.critical_celsius, Some(100.0));
    }

    /// Integration: never panics, and any readings are plausible.
    #[test]
    fn samples_real_sensors() {
        let mut t = SensorTracker::new();
        let (gpus, temps) = t.sample(Instant::now());
        assert!(temps.iter().all(|r| r.celsius > 0.0 && r.celsius < 200.0));
        assert!(gpus.iter().all(|g| !g.id.is_empty() && !g.name.is_empty()));
        // A second call within REFRESH repeats cached values.
        let (gpus2, temps2) = t.sample(Instant::now());
        assert_eq!(gpus.len(), gpus2.len());
        assert_eq!(temps.len(), temps2.len());
    }
}
