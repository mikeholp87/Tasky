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

fn unsupported<T>(what: &str) -> crate::errors::AppResult<T> {
    Err(crate::errors::AppError::UnsupportedOperation(
        what.to_owned(),
    ))
}

pub fn sockets() -> crate::errors::AppResult<crate::models::SocketList> {
    unsupported("The ports view")
}

pub fn open_files(_pid: u32) -> crate::errors::AppResult<crate::models::OpenFileList> {
    unsupported("Listing open files")
}

pub fn find_file_users(_path: &str) -> crate::errors::AppResult<crate::models::FileUserList> {
    unsupported("Finding processes that use a file")
}

pub fn memory_map(_pid: u32) -> crate::errors::AppResult<crate::models::MemoryMap> {
    unsupported("The memory map")
}

pub fn modules(_pid: u32) -> crate::errors::AppResult<Vec<crate::models::ModuleInfo>> {
    unsupported("Listing modules")
}

pub fn threads(_pid: u32) -> crate::errors::AppResult<Vec<crate::inspect::RawThread>> {
    unsupported("Listing threads")
}

pub fn perform_action(
    _pid: u32,
    _start_time: u64,
    _action: crate::models::ProcessAction,
) -> crate::errors::AppResult<()> {
    unsupported("Controlling processes")
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
