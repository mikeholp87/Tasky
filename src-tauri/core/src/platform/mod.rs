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
pub mod linux_facts;
#[cfg(any(target_os = "linux", test))]
pub mod procfs;

#[cfg(target_os = "linux")]
use linux as imp;
#[cfg(target_os = "macos")]
use macos as imp;
#[cfg(target_os = "windows")]
use windows as imp;

// Facts only Linux implements; other platforms get "unknown" defaults.
#[cfg(not(target_os = "linux"))]
use fallback as linux_only;
#[cfg(target_os = "linux")]
use linux as linux_only;

/// OS and session facts sysinfo does not report. All `None` where the
/// concept does not apply (e.g. display server on Windows).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OsDetails {
    pub distribution: Option<String>,
    pub desktop_environment: Option<String>,
    pub display_server: Option<String>,
    pub container: Option<String>,
    pub init_system: Option<String>,
}

/// Read once at startup.
pub fn os_details() -> OsDetails {
    linux_only::os_details()
}

/// Pins the calling thread to the `slot`-th CPU the process may use. `false`
/// where unsupported or refused; never fatal.
pub fn pin_current_thread(slot: usize) -> bool {
    linux_only::pin_current_thread(slot)
}

/// AC or battery where the OS says so.
pub fn power_source() -> crate::benchmarks::PowerSource {
    linux_only::power_source()
}

/// The CPU frequency governor, where one exists.
pub fn cpu_governor() -> Option<String> {
    linux_only::cpu_governor()
}

/// Running under a hypervisor, where the OS can tell.
pub fn is_virtualized() -> bool {
    linux_only::is_virtualized()
}

/// Proportional set size (shared pages split between the processes sharing
/// them) in bytes. `None` where the OS has no such figure or denies access.
pub fn proportional_memory(pid: u32) -> Option<u64> {
    linux_only::proportional_memory(pid)
}

/// Kind of a network interface; `Unknown` where the OS gives no reliable
/// signal. Callers cache it per interface name.
pub fn interface_kind(name: &str) -> crate::models::InterfaceKind {
    linux_only::interface_kind(name)
}

/// Machine identity, CPU details and batteries beyond what sysinfo reports.
/// Fields the platform cannot supply are empty.
pub fn hardware_info() -> crate::models::HardwareInfo {
    linux_only::hardware_info()
}

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

// ------------------------------------------------------------- inspection
// On-demand, read-only views of one process or of the whole machine. Each
// call may be expensive (it walks /proc or OS tables), so callers run it on a
// blocking worker, only while the matching UI is visible.

/// Every TCP/UDP socket with its owning process.
pub fn sockets() -> crate::errors::AppResult<crate::models::SocketList> {
    imp::sockets()
}

/// Files, directories and other objects `pid` holds open.
pub fn open_files(pid: u32) -> crate::errors::AppResult<crate::models::OpenFileList> {
    imp::open_files(pid)
}

/// Processes using `path` (a file or, where supported, a directory).
pub fn find_file_users(path: &str) -> crate::errors::AppResult<crate::models::FileUserList> {
    imp::find_file_users(path)
}

/// Address-space layout of `pid`.
pub fn memory_map(pid: u32) -> crate::errors::AppResult<crate::models::MemoryMap> {
    imp::memory_map(pid)
}

/// Executable and libraries loaded into `pid`.
pub fn modules(pid: u32) -> crate::errors::AppResult<Vec<crate::models::ModuleInfo>> {
    imp::modules(pid)
}

/// Threads of `pid` with cumulative CPU time.
pub fn threads(pid: u32) -> crate::errors::AppResult<Vec<crate::inspect::RawThread>> {
    imp::threads(pid)
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

// Defaults for platforms without an implementation of the Linux-only
// facts above.
#[cfg(not(target_os = "linux"))]
mod fallback {
    pub fn os_details() -> super::OsDetails {
        super::OsDetails::default()
    }

    pub fn interface_kind(_name: &str) -> crate::models::InterfaceKind {
        crate::models::InterfaceKind::Unknown
    }

    pub fn proportional_memory(_pid: u32) -> Option<u64> {
        None
    }

    pub fn pin_current_thread(_slot: usize) -> bool {
        false
    }

    pub fn power_source() -> crate::benchmarks::PowerSource {
        crate::benchmarks::PowerSource::Unknown
    }

    pub fn cpu_governor() -> Option<String> {
        None
    }

    pub fn is_virtualized() -> bool {
        false
    }

    pub fn hardware_info() -> crate::models::HardwareInfo {
        crate::models::HardwareInfo::default()
    }
}
