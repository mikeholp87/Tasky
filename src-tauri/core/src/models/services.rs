use serde::{Deserialize, Serialize};

/// Which systemd instance to talk to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum ServiceScope {
    /// System services (need authorization to change).
    System,
    /// The current user's own services.
    User,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum ServiceAction {
    Start,
    Stop,
    Restart,
}

/// One systemd service (Linux).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ServiceInfo {
    /// Unit name including the suffix, e.g. `cups.service`. Passed back to
    /// `control_service`.
    pub unit: String,
    pub description: String,
    /// systemd active state: active, inactive, failed, activating, ...
    pub active_state: String,
    /// Finer state: running, exited, dead, failed, ...
    pub sub_state: String,
    /// Whether it starts at boot (the unit file state): enabled, disabled,
    /// static, masked, ...
    pub startup: Option<String>,
    /// Main process, when one is running.
    pub main_pid: Option<u32>,
    /// Set for services the system depends on; they are shown but cannot be
    /// stopped or restarted from the app.
    pub protected: bool,
}
