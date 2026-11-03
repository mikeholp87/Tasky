use serde::Serialize;

/// Information that does not change while the app runs. Collected once at
/// startup and fetched by the frontend with a single command.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct StaticSystemInfo {
    pub platform: Platform,
    pub os_name: Option<String>,
    pub os_version: Option<String>,
    pub long_os_version: Option<String>,
    pub kernel_version: Option<String>,
    pub hostname: Option<String>,
    pub architecture: String,
    pub cpu_brand: String,
    pub cpu_vendor: String,
    pub physical_core_count: Option<u32>,
    pub logical_core_count: u32,
    pub total_memory_bytes: u64,
    /// Unix timestamp (seconds) of the last boot.
    pub boot_time_secs: Option<u64>,
    pub app_version: String,
    /// PID of this task manager process, so the UI can mark it.
    pub self_pid: u32,
    /// Linux: os-release `PRETTY_NAME`, e.g. "Debian GNU/Linux 13 (trixie)".
    pub distribution: Option<String>,
    /// Linux: desktop environment from the session, e.g. "KDE Plasma".
    pub desktop_environment: Option<String>,
    /// Linux: "Wayland" or "X11" for the user's session.
    pub display_server: Option<String>,
    /// Container or compatibility layer the app runs in, e.g. "Docker", "WSL".
    pub container: Option<String>,
    /// Linux: name of PID 1, e.g. "systemd".
    pub init_system: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum Platform {
    Windows,
    Linux,
    Macos,
    Other,
}

impl Platform {
    pub const fn current() -> Self {
        if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "linux") {
            Self::Linux
        } else if cfg!(target_os = "macos") {
            Self::Macos
        } else {
            Self::Other
        }
    }
}
