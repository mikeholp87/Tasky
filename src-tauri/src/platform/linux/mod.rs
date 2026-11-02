//! Linux implementation, backed by `/proc`.

use std::collections::HashMap;

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
                },
            ))
        })
        .collect()
}

/// sysinfo's user list resolves Unix uids via the passwd database already.
pub fn account_name(_uid: &sysinfo::Uid) -> Option<String> {
    None
}
