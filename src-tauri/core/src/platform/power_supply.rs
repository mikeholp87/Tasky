//! Linux `/sys/class/power_supply` interpretation, kept pure so it is
//! tested on every platform.

use crate::benchmarks::models::{PowerSource, PowerState};

/// One supply directory: its `type`, and `online` / `capacity` / `status`
/// file contents when present.
#[derive(Debug, Default, Clone)]
pub struct Supply {
    pub kind: String,
    pub online: Option<String>,
    pub capacity: Option<String>,
    pub status: Option<String>,
}

pub fn classify(supplies: &[Supply]) -> PowerState {
    let batteries: Vec<&Supply> = supplies
        .iter()
        .filter(|s| s.kind.trim() == "Battery")
        .collect();
    let mains_online = supplies
        .iter()
        .filter(|s| matches!(s.kind.trim(), "Mains" | "USB" | "USB_C" | "USB_PD"))
        .filter_map(|s| s.online.as_deref())
        .map(|v| v.trim() == "1")
        .reduce(|a, b| a || b);
    let discharging = batteries
        .iter()
        .any(|b| b.status.as_deref().map(str::trim) == Some("Discharging"));
    let source = match (mains_online, batteries.is_empty()) {
        (Some(true), _) => PowerSource::Ac,
        (Some(false), false) => PowerSource::Battery,
        (None, false) if discharging => PowerSource::Battery,
        (None, false) => PowerSource::Ac,
        // Desktops usually expose no supplies at all.
        _ => PowerSource::Unknown,
    };
    let battery_percent = batteries
        .iter()
        .filter_map(|b| b.capacity.as_deref()?.trim().parse::<f32>().ok())
        .find(|p| (0.0..=100.0).contains(p));
    PowerState {
        source,
        battery_percent,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn supply(
        kind: &str,
        online: Option<&str>,
        capacity: Option<&str>,
        status: Option<&str>,
    ) -> Supply {
        Supply {
            kind: kind.into(),
            online: online.map(Into::into),
            capacity: capacity.map(Into::into),
            status: status.map(Into::into),
        }
    }

    #[test]
    fn laptop_on_battery() {
        let s = classify(&[
            supply("Mains", Some("0\n"), None, None),
            supply("Battery", None, Some("57\n"), Some("Discharging\n")),
        ]);
        assert_eq!(s.source, PowerSource::Battery);
        assert_eq!(s.battery_percent, Some(57.0));
    }

    #[test]
    fn laptop_plugged_in() {
        let s = classify(&[
            supply("Mains", Some("1"), None, None),
            supply("Battery", None, Some("100"), Some("Full")),
        ]);
        assert_eq!(s.source, PowerSource::Ac);
    }

    #[test]
    fn desktop_without_supplies_is_unknown() {
        assert_eq!(classify(&[]).source, PowerSource::Unknown);
    }

    #[test]
    fn battery_only_uses_status() {
        let s = classify(&[supply("Battery", None, Some("40"), Some("Discharging"))]);
        assert_eq!(s.source, PowerSource::Battery);
        let s = classify(&[supply("Battery", None, Some("40"), Some("Charging"))]);
        assert_eq!(s.source, PowerSource::Ac);
    }
}
