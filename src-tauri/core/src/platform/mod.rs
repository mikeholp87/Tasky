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
pub mod power_supply;
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
pub fn power_source() -> crate::benchmarks::models::PowerSource {
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
    #[cfg(target_os = "windows")]
    return windows::interface_kind(name);
    #[cfg(not(target_os = "windows"))]
    linux_only::interface_kind(name)
}

/// Machine identity, CPU details and batteries beyond what sysinfo reports.
/// Fields the platform cannot supply are empty.
pub fn hardware_info() -> crate::models::HardwareInfo {
    #[cfg(target_os = "windows")]
    return windows::hardware_info();
    #[cfg(not(target_os = "windows"))]
    linux_only::hardware_info()
}

/// Programs that start at login, from Windows Run/Startup or Linux XDG entries.
pub fn startup_items() -> crate::errors::AppResult<Vec<crate::models::StartupItem>> {
    #[cfg(target_os = "windows")]
    return windows::startup_items();
    #[cfg(not(target_os = "windows"))]
    linux_only::startup_items()
}

/// Enables or disables one startup program, reversibly.
pub fn set_startup_enabled(id: &str, enabled: bool) -> crate::errors::AppResult<()> {
    #[cfg(target_os = "windows")]
    return windows::set_startup_enabled(id, enabled);
    #[cfg(not(target_os = "windows"))]
    linux_only::set_startup_enabled(id, enabled)
}

/// Memory figures that sysinfo does not expose but the OS reports reliably.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MemoryExtras {
    pub cached_bytes: Option<u64>,
    pub buffers_bytes: Option<u64>,
    pub commit_used_bytes: Option<u64>,
    pub commit_limit_bytes: Option<u64>,
    pub kernel_paged_bytes: Option<u64>,
    pub kernel_nonpaged_bytes: Option<u64>,
}

/// AC/battery state where the OS reports it; `Unknown` otherwise.
pub fn power_state() -> crate::benchmarks::models::PowerState {
    imp::power_state()
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

/// Performs a native action only after re-reading the live process identity.
/// The native pin stays alive through the action (Windows handle / Linux pidfd).
pub(crate) fn perform_action(
    pid: u32,
    start_time: u64,
    action: crate::models::ProcessAction,
) -> crate::errors::AppResult<()> {
    let pin = imp::pin_process(pid)?;
    verify_process_identity(pid, start_time)?;
    pin.perform(action)
}

pub fn verify_process_identity(pid: u32, start_time: u64) -> crate::errors::AppResult<()> {
    use crate::errors::AppError;
    if pid == 0 || start_time == 0 {
        return Err(AppError::ProcessNotFound(pid));
    }
    let mut sys = sysinfo::System::new().map_err(|e| AppError::SystemQueryFailed(e.to_string()))?;
    let target = [sysinfo::Pid::from_u32(pid)];
    sys.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::Some(&target),
        true,
        sysinfo::ProcessRefreshKind::nothing().without_tasks(),
    );
    if sys
        .process(target[0])
        .is_some_and(|p| p.start_time() == start_time)
    {
        Ok(())
    } else {
        Err(AppError::ProcessNotFound(pid))
    }
}

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

/// Opens a storage-benchmark file. `direct` requests unbuffered I/O
/// (Windows `FILE_FLAG_NO_BUFFERING | FILE_FLAG_WRITE_THROUGH`, Linux
/// `O_DIRECT`); it fails where unsupported so the caller can fall back.
/// `create` uses create-new semantics: an existing file is never reused.
pub fn open_benchmark_file(
    path: &std::path::Path,
    create: bool,
    direct: bool,
) -> std::io::Result<std::fs::File> {
    imp::open_benchmark_file(path, create, direct)
}

/// Reads exactly `buf.len()` bytes at `offset`.
pub fn read_exact_at(
    file: &std::fs::File,
    mut buf: &mut [u8],
    mut offset: u64,
) -> std::io::Result<()> {
    while !buf.is_empty() {
        match imp::read_at(file, buf, offset)? {
            0 => return Err(std::io::ErrorKind::UnexpectedEof.into()),
            n => {
                buf = &mut buf[n..];
                offset += n as u64;
            }
        }
    }
    Ok(())
}

/// Writes all of `buf` at `offset`.
pub fn write_all_at(file: &std::fs::File, mut buf: &[u8], mut offset: u64) -> std::io::Result<()> {
    while !buf.is_empty() {
        match imp::write_at(file, buf, offset)? {
            0 => return Err(std::io::ErrorKind::WriteZero.into()),
            n => {
                buf = &buf[n..];
                offset += n as u64;
            }
        }
    }
    Ok(())
}

// Defaults for platforms without an implementation of the Linux-only
// facts above.
#[cfg(not(target_os = "linux"))]
mod fallback {
    pub fn os_details() -> super::OsDetails {
        super::OsDetails::default()
    }

    #[cfg(not(target_os = "windows"))]
    pub fn interface_kind(_name: &str) -> crate::models::InterfaceKind {
        crate::models::InterfaceKind::Unknown
    }

    pub fn proportional_memory(_pid: u32) -> Option<u64> {
        None
    }

    pub fn pin_current_thread(_slot: usize) -> bool {
        false
    }

    pub fn power_source() -> crate::benchmarks::models::PowerSource {
        crate::benchmarks::models::PowerSource::Unknown
    }

    pub fn cpu_governor() -> Option<String> {
        None
    }

    pub fn is_virtualized() -> bool {
        false
    }

    #[cfg(not(target_os = "windows"))]
    pub fn hardware_info() -> crate::models::HardwareInfo {
        crate::models::HardwareInfo::default()
    }

    #[cfg(not(target_os = "windows"))]
    pub fn startup_items() -> crate::errors::AppResult<Vec<crate::models::StartupItem>> {
        Err(unsupported())
    }

    #[cfg(not(target_os = "windows"))]
    pub fn set_startup_enabled(_id: &str, _enabled: bool) -> crate::errors::AppResult<()> {
        Err(unsupported())
    }

    #[cfg(not(target_os = "windows"))]
    fn unsupported() -> crate::errors::AppError {
        crate::errors::AppError::UnsupportedOperation("Startup items".into())
    }
}

/// External-drive and service operations use the native backend on Windows.
#[cfg(windows)]
pub use windows::{
    control_service, mount_volume, power_off_volume, service_caution, services, unmount_volume,
    volumes,
};
#[cfg(not(windows))]
mod management_fallback {
    use crate::{
        errors::{AppError, AppResult},
        models::{ServiceAction, ServiceInfo, ServiceScope, Volume},
    };
    fn unsupported() -> AppError {
        AppError::UnsupportedOperation("Drive and service management".into())
    }
    pub fn volumes() -> AppResult<Vec<Volume>> {
        Err(unsupported())
    }
    pub fn mount_volume(_: &str) -> AppResult<Option<String>> {
        Err(unsupported())
    }
    pub fn unmount_volume(_: &str) -> AppResult<()> {
        Err(unsupported())
    }
    pub fn power_off_volume(_: &str) -> AppResult<()> {
        Err(unsupported())
    }
    pub fn service_caution(_: &str) -> Option<&'static str> {
        None
    }
    pub fn services(_: ServiceScope) -> AppResult<Vec<ServiceInfo>> {
        Err(unsupported())
    }
    pub fn control_service(
        _: ServiceScope,
        _: &str,
        _: ServiceAction,
        _: &dyn Fn(&ServiceInfo) -> AppResult<()>,
    ) -> AppResult<()> {
        Err(unsupported())
    }
}
#[cfg(not(windows))]
pub use management_fallback::*;
