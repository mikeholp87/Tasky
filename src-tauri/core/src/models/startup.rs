use serde::Serialize;

/// Where a startup entry lives. System entries are read-only for ordinary
/// accounts; disabling one writes a per-user override instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum StartupScope {
    User,
    System,
}

/// One program that starts when the user logs in (Linux: XDG autostart).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct StartupItem {
    /// Stable key passed back to `set_startup_enabled` (the `.desktop` file name).
    pub id: String,
    pub name: String,
    pub comment: Option<String>,
    pub command: String,
    pub scope: StartupScope,
    pub enabled: bool,
    /// Only runs in certain desktops (`OnlyShowIn`), e.g. "GNOME".
    pub only_in: Option<String>,
    /// Whether this desktop would start it at all (`OnlyShowIn` / `NotShowIn`
    /// against the current desktop). `true` when the desktop is unknown.
    pub runs_here: bool,
}
