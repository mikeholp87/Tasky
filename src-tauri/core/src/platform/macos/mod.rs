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

pub fn power_state() -> crate::benchmarks::models::PowerState {
    crate::benchmarks::models::PowerState::default()
}

/// macOS direct I/O (`F_NOCACHE`) is not implemented yet: only buffered
/// access is offered, and results are labelled accordingly.
pub fn open_benchmark_file(
    path: &std::path::Path,
    create: bool,
    direct: bool,
) -> std::io::Result<std::fs::File> {
    if direct {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "direct I/O is not implemented on macOS",
        ));
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true);
    if create {
        options.create_new(true);
    }
    options.open(path)
}

pub fn read_at(file: &std::fs::File, buf: &mut [u8], offset: u64) -> std::io::Result<usize> {
    std::os::unix::fs::FileExt::read_at(file, buf, offset)
}

pub fn write_at(file: &std::fs::File, buf: &[u8], offset: u64) -> std::io::Result<usize> {
    std::os::unix::fs::FileExt::write_at(file, buf, offset)
}
