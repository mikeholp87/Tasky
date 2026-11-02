//! Linux implementation, backed by `/proc`.

use super::{MemoryExtras, procfs};

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
