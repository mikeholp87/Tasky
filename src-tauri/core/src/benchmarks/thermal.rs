//! Temperature observation around heavy benchmarks.
//!
//! Sensors come from sysinfo's component list (Linux hwmon; on Windows,
//! WMI thermal zones that are usually unavailable to unelevated apps).
//! When no sensor reports, nothing is invented: results say
//! "Temperature monitoring unavailable on this system".

use sysinfo::Components;

use super::models::{TemperatureReading, ThermalObservation};

/// Labels that identify CPU package/core sensors across drivers.
const CPU_SENSOR_HINTS: [&str; 7] = [
    "cpu", "package", "tctl", "tdie", "coretemp", "k10temp", "core",
];

pub struct ThermalProbe {
    components: Option<Components>,
}

impl ThermalProbe {
    pub fn new() -> Self {
        Self {
            components: Components::new_with_refreshed_list().ok(),
        }
    }

    /// All readable sensors right now.
    pub fn readings(&mut self) -> Vec<TemperatureReading> {
        let Some(components) = self.components.as_mut() else {
            return Vec::new();
        };
        components.refresh(false);
        components
            .iter()
            .filter_map(|c| {
                let celsius = c.temperature()?;
                (celsius.is_finite() && celsius > -40.0 && celsius < 150.0).then(|| {
                    TemperatureReading {
                        label: c.label().to_owned(),
                        celsius,
                        critical_celsius: c.critical().filter(|v| v.is_finite() && *v > 0.0),
                    }
                })
            })
            .collect()
    }

    /// Hottest CPU-like sensor, if any.
    pub fn cpu(&mut self) -> Option<TemperatureReading> {
        hottest_cpu(self.readings())
    }
}

impl Default for ThermalProbe {
    fn default() -> Self {
        Self::new()
    }
}

pub fn hottest_cpu(readings: Vec<TemperatureReading>) -> Option<TemperatureReading> {
    readings
        .into_iter()
        .filter(|r| {
            let label = r.label.to_ascii_lowercase();
            CPU_SENSOR_HINTS.iter().any(|h| label.contains(h))
        })
        .max_by(|a, b| a.celsius.total_cmp(&b.celsius))
}

/// Accumulates before/peak/after temperatures and observed frequencies.
#[derive(Debug, Default)]
pub struct ThermalTracker {
    sensor: Option<String>,
    before: Option<f32>,
    peak: Option<f32>,
    last: Option<f32>,
    critical: Option<f32>,
    min_mhz: Option<u64>,
    max_mhz: Option<u64>,
}

impl ThermalTracker {
    pub fn record(&mut self, reading: Option<&TemperatureReading>, frequency_mhz: Option<u64>) {
        if let Some(r) = reading {
            self.sensor.get_or_insert_with(|| r.label.clone());
            self.before.get_or_insert(r.celsius);
            self.peak = Some(self.peak.map_or(r.celsius, |p| p.max(r.celsius)));
            self.last = Some(r.celsius);
            if r.critical_celsius.is_some() {
                self.critical = r.critical_celsius;
            }
        }
        if let Some(f) = frequency_mhz.filter(|f| *f > 0) {
            self.min_mhz = Some(self.min_mhz.map_or(f, |m| m.min(f)));
            self.max_mhz = Some(self.max_mhz.map_or(f, |m| m.max(f)));
        }
    }

    pub fn observation(&self) -> Option<ThermalObservation> {
        if self.sensor.is_none() && self.min_mhz.is_none() {
            return None;
        }
        Some(ThermalObservation {
            sensor: self.sensor.clone().unwrap_or_default(),
            before_celsius: self.before,
            peak_celsius: self.peak,
            after_celsius: self.last,
            critical_celsius: self.critical,
            min_frequency_mhz: self.min_mhz,
            max_frequency_mhz: self.max_mhz,
        })
    }

    pub fn has_temperature(&self) -> bool {
        self.sensor.is_some()
    }

    /// Peak came within 5 °C of an OS-reported critical threshold. Without a
    /// reported threshold no judgement is made (limits vary by hardware).
    pub fn near_critical(&self) -> bool {
        matches!((self.peak, self.critical), (Some(p), Some(c)) if p >= c - 5.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(label: &str, celsius: f32, critical: Option<f32>) -> TemperatureReading {
        TemperatureReading {
            label: label.into(),
            celsius,
            critical_celsius: critical,
        }
    }

    #[test]
    fn picks_hottest_cpu_sensor() {
        let picked = hottest_cpu(vec![
            r("nvme Composite", 90.0, None),
            r("coretemp Package id 0", 61.0, Some(100.0)),
            r("coretemp Core 3", 64.0, Some(100.0)),
            r("acpitz temp1", 40.0, None),
        ])
        .unwrap();
        assert_eq!(picked.label, "coretemp Core 3");
        assert!(hottest_cpu(vec![r("nvme", 50.0, None)]).is_none());
    }

    #[test]
    fn tracks_before_peak_after_and_frequency() {
        let mut t = ThermalTracker::default();
        assert!(t.observation().is_none());
        t.record(Some(&r("cpu", 45.0, Some(100.0))), Some(3_000));
        t.record(Some(&r("cpu", 97.0, Some(100.0))), Some(4_200));
        t.record(Some(&r("cpu", 70.0, Some(100.0))), Some(2_800));
        let o = t.observation().unwrap();
        assert_eq!(o.before_celsius, Some(45.0));
        assert_eq!(o.peak_celsius, Some(97.0));
        assert_eq!(o.after_celsius, Some(70.0));
        assert_eq!(o.min_frequency_mhz, Some(2_800));
        assert_eq!(o.max_frequency_mhz, Some(4_200));
        assert!(t.near_critical());
    }

    #[test]
    fn no_threshold_means_no_judgement() {
        let mut t = ThermalTracker::default();
        t.record(Some(&r("cpu", 99.0, None)), None);
        assert!(!t.near_critical());
        assert!(t.has_temperature());
    }
}
