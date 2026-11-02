use serde::{Deserialize, Serialize};

use crate::errors::{AppError, AppResult};

/// What an alert rule watches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum AlertMetric {
    /// Whole-system CPU usage, percent.
    SystemCpu,
    /// Physical memory in use, percent.
    SystemMemory,
    /// Capacity used on any drive, percent.
    DiskUsage,
    /// CPU of a matching process, percent of the whole machine.
    ProcessCpu,
    /// Resident memory of a matching process, bytes.
    ProcessMemory,
}

impl AlertMetric {
    pub fn is_process(self) -> bool {
        matches!(self, Self::ProcessCpu | Self::ProcessMemory)
    }

    pub fn is_percent(self) -> bool {
        !matches!(self, Self::ProcessMemory)
    }
}

/// "IF metric > threshold FOR duration THEN notify".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AlertRule {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub metric: AlertMetric,
    pub threshold: f64,
    /// How long the condition must hold before the alert fires.
    pub duration_secs: u32,
    /// Process rules only: case-insensitive name match, e.g. "node".
    /// A trailing ".exe" is ignored so rules work across platforms.
    pub process_name: Option<String>,
}

pub const MAX_ALERT_RULES: usize = 50;
pub const MAX_ALERT_DURATION_SECS: u32 = 3_600;

impl AlertRule {
    pub fn validate(&self) -> AppResult<()> {
        let bad = |msg: &str| {
            Err(AppError::InvalidArgument(format!(
                "Alert \"{}\": {msg}",
                self.name
            )))
        };
        if self.id.is_empty() || self.id.len() > 64 {
            return bad("invalid identifier.");
        }
        if self.name.trim().is_empty() || self.name.len() > 100 {
            return bad("name must be 1–100 characters.");
        }
        if !self.threshold.is_finite() || self.threshold < 0.0 {
            return bad("threshold must be a positive number.");
        }
        if self.metric.is_percent() && self.threshold > 100.0 {
            return bad("percentage thresholds must be at most 100.");
        }
        if self.duration_secs > MAX_ALERT_DURATION_SECS {
            return bad("duration must be at most one hour.");
        }
        match (&self.process_name, self.metric.is_process()) {
            (None, true) => return bad("process alerts need a process name."),
            (Some(n), true) if n.trim().is_empty() || n.len() > 260 => {
                return bad("process name must be 1–260 characters.");
            }
            _ => {}
        }
        Ok(())
    }
}

/// A fired alert, shown in the app and as a desktop notification.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AlertEvent {
    pub rule_id: String,
    pub rule_name: String,
    /// Unix milliseconds.
    pub timestamp_ms: u64,
    pub message: String,
    pub pid: Option<u32>,
}

/// Sensible starting rules, mirroring the examples in the product brief.
pub fn default_alert_rules() -> Vec<AlertRule> {
    let rule = |id: &str, name: &str, metric, threshold, duration_secs, enabled| AlertRule {
        id: id.into(),
        name: name.into(),
        enabled,
        metric,
        threshold,
        duration_secs,
        process_name: None,
    };
    vec![
        rule(
            "cpu-high",
            "CPU above 90% for 30 s",
            AlertMetric::SystemCpu,
            90.0,
            30,
            true,
        ),
        rule(
            "memory-high",
            "Memory above 85%",
            AlertMetric::SystemMemory,
            85.0,
            10,
            true,
        ),
        rule(
            "disk-full",
            "Drive more than 95% full",
            AlertMetric::DiskUsage,
            95.0,
            0,
            true,
        ),
    ]
}
