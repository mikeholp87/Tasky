use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use sysinfo::{DiskRefreshKind, Disks};

use crate::models::{DiskKind, DiskSnapshot};
use crate::monitoring::rates::{active_percent, counter_rate, delta_rate, percent};
use crate::platform::{self, DiskIoCounters};

/// Fallback cadence for re-enumerating the mount list. On Linux a change to
/// the mount table triggers it at once (see [`mount_table`]), so this only
/// covers other platforms and the slow-moving disk type. Capacities are
/// re-read on every tick (a `statvfs`/`GetDiskFreeSpaceEx` per mount) so
/// deleting files is reflected immediately in the UI.
const MOUNT_LIST_REFRESH: Duration = Duration::from_secs(30);

/// The kernel's mount table. Reading it is a few KB, so it is compared on
/// every tick: a drive that was just plugged in or unmounted shows up on the
/// next tick instead of after the fallback interval.
#[cfg(target_os = "linux")]
fn mount_table() -> Option<String> {
    std::fs::read_to_string("/proc/self/mountinfo").ok()
}

#[cfg(windows)]
fn mount_table() -> Option<String> {
    // SAFETY: GetLogicalDrives has no pointer parameters. A failed query
    // falls back to the periodic refresh rather than clearing the list.
    let drives = unsafe { windows::Win32::Storage::FileSystem::GetLogicalDrives() };
    (drives != 0).then(|| drives.to_string())
}

#[cfg(not(any(target_os = "linux", windows)))]
fn mount_table() -> Option<String> {
    None
}

/// Whether to re-enumerate drives: the mount table changed (when readable),
/// or the fallback interval has elapsed.
fn list_refresh_due(
    current: Option<&str>,
    previous: Option<&str>,
    since_refresh: Duration,
) -> bool {
    since_refresh >= MOUNT_LIST_REFRESH || (current.is_some() && current != previous)
}

/// Pseudo / image file systems that are not useful as "drives".
const IGNORED_FILE_SYSTEMS: &[&str] = &[
    "squashfs",
    "overlay",
    "tmpfs",
    "devtmpfs",
    "ramfs",
    "proc",
    "sysfs",
    "efivarfs",
    "autofs",
    "cgroup",
    "cgroup2",
    "securityfs",
    "debugfs",
    "tracefs",
    "pstore",
    "configfs",
    "fusectl",
    "bpf",
    "mqueue",
    "hugetlbfs",
    "binfmt_misc",
    "nsfs",
    "devpts",
    "fuse.portal",
    "fuse.gvfsd-fuse",
];

pub struct DiskTracker {
    /// `None` when the platform cannot enumerate disks at all.
    disks: Option<Disks>,
    last_mount_refresh: Instant,
    /// Mount table at the last re-enumeration.
    mount_table: Option<String>,
    /// Mount points that already have a byte-count baseline. A disk's first
    /// sample reports lifetime totals rather than a delta, so it shows 0.
    primed: HashSet<String>,
    /// Previous operation/busy counters per mount point.
    previous_io: HashMap<String, DiskIoCounters>,
}

impl DiskTracker {
    pub fn new() -> Self {
        let disks = Disks::new_with_refreshed_list_specifics(DiskRefreshKind::everything())
            .inspect_err(|e| tracing::warn!("disk enumeration unavailable: {e}"))
            .ok();
        Self {
            disks,
            last_mount_refresh: Instant::now(),
            mount_table: mount_table(),
            primed: HashSet::new(),
            previous_io: HashMap::new(),
        }
    }

    pub fn sample(&mut self, now: Instant, elapsed: Duration) -> Vec<DiskSnapshot> {
        let Some(disks) = self.disks.as_mut() else {
            return Vec::new();
        };
        let table = mount_table();
        let since = now.duration_since(self.last_mount_refresh);
        if list_refresh_due(table.as_deref(), self.mount_table.as_deref(), since) {
            disks.refresh_specifics(true, DiskRefreshKind::everything());
            self.last_mount_refresh = now;
            self.mount_table = table;
        } else {
            // Free space is cheap to query and changes when the user writes or
            // deletes files, so refresh it on every tick.
            disks.refresh_specifics(false, tick_refresh_kind());
        }

        let relevant: Vec<&sysinfo::Disk> = disks
            .iter()
            .filter(|d| {
                is_relevant(
                    &d.file_system().to_string_lossy(),
                    &d.mount_point().to_string_lossy(),
                    d.total_space(),
                )
            })
            .collect();

        let pairs: Vec<(String, String)> = relevant
            .iter()
            .map(|d| {
                (
                    d.name().to_string_lossy().into_owned(),
                    d.mount_point().to_string_lossy().into_owned(),
                )
            })
            .collect();
        let io_now = platform::disk_io_counters(&pairs);

        let mut primed = HashSet::with_capacity(relevant.len());
        let snapshots = relevant
            .iter()
            .map(|d| {
                let mount_point = d.mount_point().to_string_lossy().into_owned();
                let has_baseline = self.primed.contains(&mount_point);
                primed.insert(mount_point.clone());

                let usage = d.usage();
                // A disk whose lifetime counters are zero does not report I/O
                // at all (e.g. some network or virtual volumes).
                let io_supported = usage.total_read_bytes > 0 || usage.total_written_bytes > 0;
                let rate = |delta: u64| {
                    io_supported.then(|| {
                        if has_baseline {
                            delta_rate(delta, elapsed)
                        } else {
                            0.0
                        }
                    })
                };

                let ops = match (self.previous_io.get(&mount_point), io_now.get(&mount_point)) {
                    (Some(prev), Some(cur)) => Some((
                        counter_rate(prev.reads, cur.reads, elapsed),
                        counter_rate(prev.writes, cur.writes, elapsed),
                        active_percent(prev, cur, elapsed),
                    )),
                    // First sample for this volume: counters known, no rate yet.
                    (None, Some(_)) => Some((0.0, 0.0, Some(0.0))),
                    _ => None,
                };

                let total = d.total_space();
                let available = d.available_space().min(total);
                DiskSnapshot {
                    name: d.name().to_string_lossy().into_owned(),
                    mount_point,
                    file_system: d.file_system().to_string_lossy().into_owned(),
                    kind: match d.kind() {
                        sysinfo::DiskKind::SSD => DiskKind::Ssd,
                        sysinfo::DiskKind::HDD => DiskKind::Hdd,
                        sysinfo::DiskKind::Unknown(_) => DiskKind::Unknown,
                    },
                    removable: d.is_removable(),
                    read_only: d.is_read_only(),
                    total_bytes: total,
                    available_bytes: available,
                    used_bytes: total - available,
                    used_percent: percent(total - available, total),
                    read_bytes_per_sec: rate(usage.read_bytes),
                    write_bytes_per_sec: rate(usage.written_bytes),
                    read_ops_per_sec: ops.map(|o| o.0),
                    write_ops_per_sec: ops.map(|o| o.1),
                    active_percent: ops.and_then(|o| o.2),
                }
            })
            .collect();
        self.primed = primed;
        self.previous_io = io_now;
        snapshots
    }
}

/// What every tick must re-read: free space (so deletions are visible
/// immediately) and I/O counters (so throughput stays live). Disk type and the
/// mount list change rarely, so they are refreshed on the slower cadence.
fn tick_refresh_kind() -> DiskRefreshKind {
    DiskRefreshKind::nothing().with_storage().with_io_usage()
}

/// Filters out pseudo file systems, snap images and zero-sized mounts.
pub fn is_relevant(file_system: &str, mount_point: &str, total_bytes: u64) -> bool {
    if total_bytes == 0 {
        return false;
    }
    let fs = file_system.to_ascii_lowercase();
    if IGNORED_FILE_SYSTEMS.contains(&fs.as_str()) {
        return false;
    }
    !(mount_point.starts_with("/snap/") || mount_point.starts_with("/var/lib/docker/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_real_drives() {
        assert!(is_relevant("NTFS", "C:\\", 500));
        assert!(is_relevant("ext4", "/", 500));
        assert!(is_relevant("vfat", "/boot/efi", 500));
    }

    #[test]
    fn every_tick_refreshes_capacity() {
        let kind = tick_refresh_kind();
        assert!(kind.storage(), "free space must update on every tick");
        assert!(kind.io_usage(), "throughput must update on every tick");
        assert!(!kind.kind(), "disk type only needs the slow refresh");
    }

    #[test]
    fn a_changed_mount_table_refreshes_the_drive_list_at_once() {
        let soon = Duration::from_secs(1);
        assert!(!list_refresh_due(Some("a"), Some("a"), soon));
        assert!(
            list_refresh_due(Some("a\nb"), Some("a"), soon),
            "drive plugged in"
        );
        assert!(
            list_refresh_due(Some("a"), Some("a\nb"), soon),
            "drive removed"
        );
        // No mount table (other platforms, unreadable): only the timer applies.
        assert!(!list_refresh_due(None, Some("a"), soon));
        assert!(list_refresh_due(None, None, MOUNT_LIST_REFRESH));
    }

    #[test]
    fn drops_pseudo_and_snap_mounts() {
        assert!(!is_relevant("squashfs", "/snap/core/1", 500));
        assert!(!is_relevant("ext4", "/snap/foo", 500));
        assert!(!is_relevant("tmpfs", "/run", 500));
        assert!(!is_relevant("overlay", "/var/lib/docker/overlay2/x", 500));
        assert!(!is_relevant("ext4", "/mnt/empty", 0));
    }

    /// Integration: the system drive reports capacity and, on Windows/Linux,
    /// operation counters.
    #[test]
    fn samples_real_disks() {
        let mut tracker = DiskTracker::new();
        tracker.sample(Instant::now(), Duration::from_secs(1));
        std::thread::sleep(Duration::from_millis(300));
        let disks = tracker.sample(Instant::now(), Duration::from_millis(300));
        assert!(!disks.is_empty(), "at least one drive");
        let d = &disks[0];
        assert!(d.total_bytes > 0 && d.used_bytes <= d.total_bytes);
        if cfg!(any(windows, target_os = "linux")) {
            assert!(
                d.read_ops_per_sec.is_some(),
                "IOPS available on {}",
                d.mount_point
            );
            assert!(d.active_percent.is_some_and(|a| (0.0..=100.0).contains(&a)));
        }
    }
}
