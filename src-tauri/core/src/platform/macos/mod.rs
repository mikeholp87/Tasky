//! macOS implementation. Not yet a supported target: functions return
//! "unavailable" so the rest of the app compiles and degrades gracefully.

use std::collections::HashMap;

use super::{MemoryExtras, ProcessExtras};

pub fn memory_extras() -> MemoryExtras {
    MemoryExtras::default()
}

pub fn process_extras(_pids: &[u32]) -> HashMap<u32, ProcessExtras> {
    HashMap::new()
}

pub fn account_name(_uid: &sysinfo::Uid) -> Option<String> {
    None
}

pub fn connections(_pid: u32) -> Option<Vec<crate::models::Connection>> {
    None
}

pub fn process_architecture(_pid: u32) -> Option<String> {
    None
}

fn unsupported(what: &str) -> crate::errors::AppResult<()> {
    Err(crate::errors::AppError::UnsupportedOperation(
        what.to_owned(),
    ))
}

pub fn terminate(_pid: u32) -> crate::errors::AppResult<()> {
    unsupported("Ending processes")
}

pub fn force_kill(_pid: u32) -> crate::errors::AppResult<()> {
    unsupported("Ending processes")
}

pub fn suspend(_pid: u32) -> crate::errors::AppResult<()> {
    unsupported("Suspending processes")
}

pub fn resume(_pid: u32) -> crate::errors::AppResult<()> {
    unsupported("Resuming processes")
}

pub fn set_priority(
    _pid: u32,
    _level: crate::models::PriorityLevel,
) -> crate::errors::AppResult<()> {
    unsupported("Changing priority")
}

pub fn is_critical(_pid: u32) -> Option<bool> {
    None
}

pub fn system_root() -> Option<String> {
    None
}

pub fn disk_io_counters(_disks: &[(String, String)]) -> HashMap<String, super::DiskIoCounters> {
    HashMap::new()
}
