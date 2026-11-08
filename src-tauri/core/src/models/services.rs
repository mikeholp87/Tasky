use serde::{Deserialize, Serialize};

/// Service manager scope. Windows SCM supports System; Linux also has User.
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

/// One background service reported by the platform service manager.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ServiceInfo {
    /// Native identifier (Windows service name or Linux unit name).
    pub unit: String,
    pub description: String,
    /// Normalized state: active, inactive, failed, activating, ...
    pub active_state: String,
    /// Finer state: running, exited, dead, failed, ...
    pub sub_state: String,
    /// Native startup policy, where readable.
    pub startup: Option<String>,
    /// Main process, when one is running.
    pub main_pid: Option<u32>,
    /// Set for services the system depends on; they are shown but cannot be
    /// stopped or restarted from the app.
    pub protected: bool,
}
