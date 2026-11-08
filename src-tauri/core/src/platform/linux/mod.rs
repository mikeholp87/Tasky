//! Linux implementation, backed by `/proc`.

mod autostart;
mod connections;
mod control;
mod hardware;
mod inspect;
mod net;
mod services;
mod system;
mod volumes;

use std::collections::HashMap;
use std::io::Read;

pub use connections::connections;
pub use control::{is_critical, perform_action, system_root};
pub use hardware::hardware_info;
pub use inspect::{find_file_users, memory_map, modules, open_files, sockets, threads};
pub use net::interface_kind;
pub use services::{control_service, services};
pub use system::os_details;
pub use volumes::{mount_volume, power_off_volume, unmount_volume, volumes};

pub fn startup_items() -> crate::errors::AppResult<Vec<crate::models::StartupItem>> {
    Ok(autostart::list(&autostart::Dirs::from_env()))
}

pub fn startup_directory() -> crate::errors::AppResult<std::path::PathBuf> {
    let directory = autostart::Dirs::from_env().user;
    if !directory.is_absolute() {
        return Err(crate::errors::AppError::InvalidArgument(
            "The startup directory must be absolute.".into(),
        ));
    }
    Ok(directory)
}

pub fn set_startup_enabled(id: &str, enabled: bool) -> crate::errors::AppResult<()> {
    autostart::set_enabled(&autostart::Dirs::from_env(), id, enabled)
}

use super::{MemoryExtras, ProcessExtras, procfs};

pub fn memory_extras() -> MemoryExtras {
    let Ok(text) = std::fs::read_to_string("/proc/meminfo") else {
        return MemoryExtras::default();
    };
    let info = procfs::parse_meminfo(&text);
    MemoryExtras {
        // Match `free`: page cache plus reclaimable slab.
        cached_bytes: info
            .cached
            .map(|c| c.saturating_add(info.s_reclaimable.unwrap_or(0))),
        buffers_bytes: info.buffers,
        commit_used_bytes: None,
        commit_limit_bytes: None,
    }
}

/// Thread count and nice value from `/proc/[pid]/stat`. A process that exits
/// between enumeration and this read is simply skipped.
pub fn process_extras(pids: &[u32]) -> HashMap<u32, ProcessExtras> {
    pids.iter()
        .filter_map(|&pid| {
            let text = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
            let stat = procfs::parse_stat(&text)?;
            Some((
                pid,
                ProcessExtras {
                    threads: Some(stat.num_threads),
                    priority: Some(stat.nice),
                    memory_bytes: None,
                    virtual_memory_bytes: None,
                },
            ))
        })
        .collect()
}

/// Pins the calling thread to the `slot`-th CPU this process may run on.
/// Only this thread's affinity changes, and it ends with the thread. `false`
/// when the system refuses (restricted containers, cgroups).
pub fn pin_current_thread(slot: usize) -> bool {
    // SAFETY: `set` is a plain bitset that is fully initialised by CPU_ZERO
    // before use; both syscalls only read or write it and the given size.
    unsafe {
        let mut allowed: libc::cpu_set_t = std::mem::zeroed();
        libc::CPU_ZERO(&mut allowed);
        let size = std::mem::size_of::<libc::cpu_set_t>();
        if libc::sched_getaffinity(0, size, &mut allowed) != 0 {
            return false;
        }
        let cpus: Vec<usize> = (0..libc::CPU_SETSIZE as usize)
            .filter(|&c| libc::CPU_ISSET(c, &allowed))
            .collect();
        let Some(&cpu) = cpus.get(slot % cpus.len().max(1)) else {
            return false;
        };
        let mut one: libc::cpu_set_t = std::mem::zeroed();
        libc::CPU_ZERO(&mut one);
        libc::CPU_SET(cpu, &mut one);
        libc::sched_setaffinity(0, size, &one) == 0
    }
}

pub fn power_source() -> crate::benchmarks::PowerSource {
    let read = |dir: &std::path::Path, f: &str| {
        std::fs::read_to_string(dir.join(f))
            .ok()
            .map(|s| s.trim().to_owned())
    };
    let supplies: Vec<super::linux_facts::PowerSupply> =
        std::fs::read_dir("/sys/class/power_supply")
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| {
                let dir = e.path();
                super::linux_facts::PowerSupply {
                    kind: read(&dir, "type").unwrap_or_default(),
                    online: read(&dir, "online").map(|v| v == "1"),
                    status: read(&dir, "status"),
                }
            })
            .collect();
    super::linux_facts::power_source(&supplies)
}

/// cpufreq governor of CPU 0, e.g. "powersave". Recorded, never changed.
pub fn cpu_governor() -> Option<String> {
    std::fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor")
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

pub fn is_virtualized() -> bool {
    std::fs::read_to_string("/proc/cpuinfo")
        .is_ok_and(|t| super::linux_facts::cpuinfo_has_hypervisor(&t))
}

/// Proportional set size of `pid`. `None` when the kernel refuses access (a
/// process of another user needs ptrace rights) or the process has exited.
pub fn proportional_memory(pid: u32) -> Option<u64> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/smaps_rollup")).ok()?;
    procfs::parse_smaps_rollup(&text).pss
}

/// sysinfo's user list resolves Unix uids via the passwd database already.
pub fn account_name(_uid: &sysinfo::Uid) -> Option<String> {
    None
}

/// Reads the ELF header of `/proc/[pid]/exe`.
pub fn process_architecture(pid: u32) -> Option<String> {
    let mut header = [0u8; 20];
    std::fs::File::open(format!("/proc/{pid}/exe"))
        .ok()?
        .read_exact(&mut header)
        .ok()?;
    procfs::elf_architecture(&header).map(str::to_owned)
}

/// Operation counts and busy time per mounted device from `/proc/diskstats`.
/// Device-mapper names (e.g. `/dev/mapper/root`) are resolved to `dm-N`.
/// Kernel name (as in `/proc/diskstats`) of the block device a mount lives
/// on, from its device number. Covers mount sources that are not a node
/// named like the kernel's, such as `/dev/root`. `None` for filesystems with
/// no single backing device (e.g. btrfs subvolumes report an anonymous one).
fn block_name_of_mount(mount: &str) -> Option<String> {
    use std::os::unix::fs::MetadataExt;
    let dev = std::fs::metadata(mount).ok()?.dev();
    let link = std::fs::read_link(format!(
        "/sys/dev/block/{}:{}",
        libc::major(dev),
        libc::minor(dev)
    ))
    .ok()?;
    Some(link.file_name()?.to_str()?.to_owned())
}

pub fn disk_io_counters(disks: &[(String, String)]) -> HashMap<String, super::DiskIoCounters> {
    let Ok(text) = std::fs::read_to_string("/proc/diskstats") else {
        return HashMap::new();
    };
    let stats = procfs::parse_diskstats(&text);
    disks
        .iter()
        .filter_map(|(device, mount)| {
            let by_node = std::fs::canonicalize(device)
                .ok()
                .and_then(|p| Some(p.file_name()?.to_str()?.to_owned()))
                .filter(|n| stats.contains_key(n.as_str()));
            let name = by_node.or_else(|| block_name_of_mount(mount))?;
            let s = stats.get(name.as_str())?;
            Some((
                mount.clone(),
                super::DiskIoCounters {
                    reads: s.reads_completed,
                    writes: s.writes_completed,
                    busy_ns: Some(s.io_ms.saturating_mul(1_000_000)),
                    idle_ns: None,
                    clock_ns: None,
                },
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mount whose source node is unusable (as `/dev/root` can be) is still
    /// matched to its disk by device number.
    #[test]
    fn disk_counters_fall_back_to_the_mount_device() {
        let known = std::fs::read_to_string("/proc/diskstats")
            .map(|t| procfs::parse_diskstats(&t))
            .unwrap_or_default();
        // Skip where `/` has no single backing disk (some containers).
        let Some(name) = block_name_of_mount("/").filter(|n| known.contains_key(n.as_str())) else {
            return;
        };
        let got = disk_io_counters(&[("/dev/no-such-node".into(), "/".into())]);
        assert!(got.contains_key("/"), "no counters for {name}");
    }
}
