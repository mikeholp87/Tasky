use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::alerts::{AlertRule, MAX_ALERT_RULES, default_alert_rules};
use super::monitoring::{DEFAULT_INTERVAL_MS, validate_interval};
use crate::errors::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

/// History lengths the UI offers, in minutes.
pub const ALLOWED_HISTORY_MINUTES: [u32; 3] = [15, 30, 60];

/// Persisted user settings. Unknown or missing fields fall back to
/// defaults, so older settings files keep working after upgrades.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    // General
    pub start_with_os: bool,
    pub minimize_to_tray: bool,
    pub launch_minimized: bool,
    pub remember_window: bool,
    // Monitoring
    pub refresh_interval_ms: u32,
    pub history_minutes: u32,
    // Appearance
    pub theme: ThemePreference,
    pub compact_table: bool,
    pub heat_mapping: bool,
    pub visible_columns: Vec<String>,
    pub column_widths: BTreeMap<String, u32>,
    /// Linux kernel threads (kworker, ksoftirqd, …) in the process list.
    pub show_kernel_threads: bool,
    /// Bridges, veth, TAP and container networks on the Network page.
    pub show_virtual_adapters: bool,
    // Alerts
    pub notifications_enabled: bool,
    pub alert_rules: Vec<AlertRule>,
    // Activity timeline
    /// A process using at least this much CPU (percent of the machine) for a
    /// few seconds is logged as a spike.
    pub cpu_spike_percent: u32,
    /// Growth in a process's memory, within half a minute, that is logged.
    pub memory_jump_mb: u32,
    /// How long a newly started process is highlighted in the process table;
    /// 0 turns the highlight off.
    pub new_process_seconds: u32,
    // Advanced
    pub debug_logging: bool,
}

/// Allowed range of [`Settings::cpu_spike_percent`].
pub const CPU_SPIKE_RANGE: std::ops::RangeInclusive<u32> = 10..=100;
/// Allowed range of [`Settings::new_process_seconds`].
pub const NEW_PROCESS_RANGE: std::ops::RangeInclusive<u32> = 0..=300;
/// Allowed range of [`Settings::memory_jump_mb`].
pub const MEMORY_JUMP_RANGE: std::ops::RangeInclusive<u32> = 64..=65_536;

impl Default for Settings {
    fn default() -> Self {
        Self {
            start_with_os: false,
            minimize_to_tray: false,
            launch_minimized: false,
            remember_window: true,
            refresh_interval_ms: DEFAULT_INTERVAL_MS,
            history_minutes: 60,
            theme: ThemePreference::System,
            compact_table: false,
            heat_mapping: true,
            // Empty means "use the UI's defaults".
            visible_columns: Vec::new(),
            column_widths: BTreeMap::new(),
            show_kernel_threads: false,
            show_virtual_adapters: false,
            notifications_enabled: true,
            alert_rules: default_alert_rules(),
            cpu_spike_percent: 80,
            memory_jump_mb: 512,
            new_process_seconds: 10,
            debug_logging: false,
        }
    }
}

impl Settings {
    /// Rejects values the app cannot honour. Called on every update from
    /// the frontend; settings files are additionally sanitized on load.
    pub fn validate(&self) -> AppResult<()> {
        validate_interval(self.refresh_interval_ms)?;
        if !ALLOWED_HISTORY_MINUTES.contains(&self.history_minutes) {
            return Err(AppError::InvalidArgument(
                "History length must be 15, 30 or 60 minutes.".into(),
            ));
        }
        if !CPU_SPIKE_RANGE.contains(&self.cpu_spike_percent) {
            return Err(AppError::InvalidArgument(
                "CPU spike threshold must be between 10 and 100 percent.".into(),
            ));
        }
        if !MEMORY_JUMP_RANGE.contains(&self.memory_jump_mb) {
            return Err(AppError::InvalidArgument(
                "Memory jump threshold must be between 64 MB and 64 GB.".into(),
            ));
        }
        if !NEW_PROCESS_RANGE.contains(&self.new_process_seconds) {
            return Err(AppError::InvalidArgument(
                "The new-process highlight must last 0 to 300 seconds.".into(),
            ));
        }
        if self.alert_rules.len() > MAX_ALERT_RULES {
            return Err(AppError::InvalidArgument(format!(
                "At most {MAX_ALERT_RULES} alert rules are supported."
            )));
        }
        let mut ids = std::collections::HashSet::new();
        for rule in &self.alert_rules {
            rule.validate()?;
            if !ids.insert(&rule.id) {
                return Err(AppError::InvalidArgument(
                    "Alert rule ids must be unique.".into(),
                ));
            }
        }
        if self.visible_columns.len() > 64 || self.column_widths.len() > 64 {
            return Err(AppError::InvalidArgument(
                "Too many column settings.".into(),
            ));
        }
        if self
            .visible_columns
            .iter()
            .chain(self.column_widths.keys())
            .any(|key| key.is_empty() || key.len() > 64)
            || self
                .column_widths
                .values()
                .any(|width| !(16..=2_000).contains(width))
        {
            return Err(AppError::InvalidArgument("Invalid column settings.".into()));
        }
        Ok(())
    }

    /// Repairs a settings file edited by hand or written by an older
    /// version: invalid scalar values revert to defaults and invalid alert
    /// rules are dropped, instead of refusing to start.
    pub fn sanitized(mut self) -> Self {
        let d = Self::default();
        if validate_interval(self.refresh_interval_ms).is_err() {
            self.refresh_interval_ms = d.refresh_interval_ms;
        }
        if !ALLOWED_HISTORY_MINUTES.contains(&self.history_minutes) {
            self.history_minutes = d.history_minutes;
        }
        if !CPU_SPIKE_RANGE.contains(&self.cpu_spike_percent) {
            self.cpu_spike_percent = d.cpu_spike_percent;
        }
        if !MEMORY_JUMP_RANGE.contains(&self.memory_jump_mb) {
            self.memory_jump_mb = d.memory_jump_mb;
        }
        if !NEW_PROCESS_RANGE.contains(&self.new_process_seconds) {
            self.new_process_seconds = d.new_process_seconds;
        }
        let mut ids = std::collections::HashSet::new();
        self.alert_rules
            .retain(|r| r.validate().is_ok() && ids.insert(r.id.clone()));
        self.alert_rules.truncate(MAX_ALERT_RULES);
        self.visible_columns.truncate(64);
        self.visible_columns
            .retain(|key| !key.is_empty() && key.len() <= 64);
        self.column_widths.retain(|key, width| {
            !key.is_empty() && key.len() <= 64 && (16..=2_000).contains(width)
        });
        while self.column_widths.len() > 64 {
            self.column_widths.pop_last();
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::AlertMetric;

    #[test]
    fn defaults_are_valid() {
        Settings::default().validate().unwrap();
    }

    #[test]
    fn oversized_column_keys_and_widths_are_rejected_and_sanitized() {
        let mut s = Settings::default();
        s.visible_columns.push("x".repeat(65));
        s.column_widths.insert("name".into(), u32::MAX);
        assert!(s.validate().is_err());
        let clean = s.sanitized();
        assert!(clean.visible_columns.is_empty() && clean.column_widths.is_empty());
        clean.validate().unwrap();
    }

    #[test]
    fn missing_fields_use_defaults() {
        let s: Settings = serde_json::from_str(r#"{ "theme": "dark" }"#).unwrap();
        assert_eq!(s.theme, ThemePreference::Dark);
        assert_eq!(s.refresh_interval_ms, DEFAULT_INTERVAL_MS);
        assert!(!s.alert_rules.is_empty());
    }

    #[test]
    fn rejects_invalid_updates() {
        let s = Settings {
            refresh_interval_ms: 3,
            ..Settings::default()
        };
        assert!(s.validate().is_err());

        let mut s = Settings::default();
        s.alert_rules[0].threshold = 250.0;
        assert!(s.validate().is_err(), "percent above 100");

        let mut s = Settings::default();
        s.alert_rules.push(AlertRule {
            id: "p".into(),
            name: "node".into(),
            enabled: true,
            metric: AlertMetric::ProcessCpu,
            threshold: 80.0,
            duration_secs: 20,
            process_name: None,
        });
        assert!(s.validate().is_err(), "process rule without a name");
    }

    #[test]
    fn sanitizes_hand_edited_files() {
        let mut s = Settings {
            history_minutes: 7,
            ..Settings::default()
        };
        s.alert_rules[1].threshold = f64::NAN;
        let clean = s.sanitized();
        assert_eq!(clean.history_minutes, 60);
        assert_eq!(clean.alert_rules.len(), 2);
        clean.validate().unwrap();
    }
}
