use serde::Serialize;

/// A drive volume the user may mount or unmount from the app. Only volumes the
/// desktop treats as user-managed (USB sticks, SD cards, external disks) are
/// ever listed; system and internal partitions are not, so the UI cannot offer
/// to unmount them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Volume {
    /// Native identity: a Windows volume GUID path or a Linux block-device path.
    pub device: String,
    pub label: Option<String>,
    pub file_system: String,
    pub size_bytes: u64,
    /// Where it is mounted now; `None` when it is not mounted.
    pub mount_point: Option<String>,
    /// Supports safe removal once every volume on the disk is unmounted.
    pub can_power_off: bool,
}
