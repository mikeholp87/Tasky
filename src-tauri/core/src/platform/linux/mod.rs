//! Linux implementation, backed by `/proc`.

mod connections;
mod control;

use std::collections::HashMap;
use std::io::Read;

pub use connections::connections;
pub use control::{force_kill, is_critical, resume, set_priority, suspend, system_root, terminate};

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
