//! Linux implementation, backed by `/proc`.

mod connections;
mod control;
mod net;
mod system;

use std::collections::HashMap;
use std::io::Read;

pub use connections::connections;
pub use control::{force_kill, is_critical, resume, set_priority, suspend, system_root, terminate};
pub use net::interface_kind;
pub use system::os_details;

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
pub fn disk_io_counters(disks: &[(String, String)]) -> HashMap<String, super::DiskIoCounters> {
    let Ok(text) = std::fs::read_to_string("/proc/diskstats") else {
        return HashMap::new();
    };
    let stats = procfs::parse_diskstats(&text);
    disks
        .iter()
        .filter_map(|(device, mount)| {
            let resolved = std::fs::canonicalize(device).ok()?;
            let name = resolved.file_name()?.to_str()?;
            let s = stats.get(name)?;
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
