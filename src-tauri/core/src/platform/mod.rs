//! Platform abstraction layer.
//!
//! Everything OS-specific lives below this module. Higher-level code calls the
//! functions re-exported here and never uses `#[cfg(target_os)]` itself.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

// Parsers for /proc files are plain functions, compiled on every platform in
// tests so they are exercised by CI on Windows too.
#[cfg(any(target_os = "linux", test))]
pub mod procfs;

#[cfg(target_os = "linux")]
use linux as imp;
#[cfg(target_os = "macos")]
use macos as imp;
#[cfg(target_os = "windows")]
use windows as imp;

/// Memory figures that sysinfo does not expose but the OS reports reliably.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MemoryExtras {
    pub cached_bytes: Option<u64>,
    pub buffers_bytes: Option<u64>,
    pub commit_used_bytes: Option<u64>,
    pub commit_limit_bytes: Option<u64>,
}

/// Best-effort memory extras; failures degrade to "unavailable".
pub fn memory_extras() -> MemoryExtras {
    imp::memory_extras()
}

/// Per-process details not exposed (cheaply) by sysinfo.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProcessExtras {
    pub threads: Option<u32>,
    /// Linux nice value or Windows base priority.
    pub priority: Option<i32>,
    /// Resident memory from a source that does not need a process handle.
    pub memory_bytes: Option<u64>,
    pub virtual_memory_bytes: Option<u64>,
}

/// Extras for the given PIDs (Windows returns all processes from one snapshot).
pub fn process_extras(pids: &[u32]) -> std::collections::HashMap<u32, ProcessExtras> {
    imp::process_extras(pids)
}

/// Account name for a user id that sysinfo's user list could not resolve
/// (e.g. Windows service accounts). Callers should cache the result.
pub fn account_name(uid: &sysinfo::Uid) -> Option<String> {
    imp::account_name(uid)
}

/// Whether the OS reports load averages. Windows has no equivalent.
pub const HAS_LOAD_AVERAGE: bool = cfg!(unix);

/// TCP/UDP sockets owned by `pid`; `None` when the OS will not say.
pub fn connections(pid: u32) -> Option<Vec<crate::models::Connection>> {
    imp::connections(pid)
}

/// Instruction set a process runs as, e.g. "x64" or "x86_64".
pub fn process_architecture(pid: u32) -> Option<String> {
    imp::process_architecture(pid)
}

// ---------------------------------------------------------------- control
// Raw OS operations. Callers must go through `process_control`, which adds
// validation, PID-reuse checks and protection rules.

pub(crate) use imp::{force_kill, resume, set_priority, suspend, terminate};

/// Windows `IsProcessCritical`; `None` elsewhere or when it cannot be read.
pub fn is_critical(pid: u32) -> Option<bool> {
    imp::is_critical(pid)
}

/// Windows `%SystemRoot%`; `None` elsewhere.
pub fn system_root() -> Option<String> {
    imp::system_root()
}

// ---------------------------------------------------------------- disk I/O

/// Cumulative per-volume I/O counters. Busy time comes either directly
/// (`busy_ns`, Linux) or as idle time plus a clock (`idle_ns`/`clock_ns`,
/// Windows), matching what each OS reports.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DiskIoCounters {
    pub reads: u64,
    pub writes: u64,
    pub busy_ns: Option<u64>,
    pub idle_ns: Option<u64>,
    pub clock_ns: Option<u64>,
}

/// Counters keyed by mount point for the given `(device name, mount point)` pairs.
pub fn disk_io_counters(
    disks: &[(String, String)],
) -> std::collections::HashMap<String, DiskIoCounters> {
    imp::disk_io_counters(disks)
}
