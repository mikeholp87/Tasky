//! On-demand inspection using documented Win32 APIs only.
//!
//! - Ports: IP Helper owner-PID tables (see `connections`).
//! - Files in use: Restart Manager, which reports applications and services
//!   holding a *file*. It cannot search directories or list every handle.
//! - Open handles of one process: not provided. Complete enumeration needs
//!   undocumented `NtQuerySystemInformation` structures, which this app does
//!   not rely on.
//! - Modules: ToolHelp. Memory map: `VirtualQueryEx`. Threads: ToolHelp.
//!
//! Written against the Microsoft documentation and checked by the compiler and
//! clippy; it has not been run on a Windows machine in this change.

use std::collections::HashMap;
use std::ffi::c_void;

use ::windows::Win32::Foundation::{
    CloseHandle, ERROR_MORE_DATA, ERROR_PARTIAL_COPY, FILETIME, HANDLE, NO_ERROR, WIN32_ERROR,
};
use ::windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, MODULEENTRY32W, Module32FirstW, Module32NextW, PROCESSENTRY32W,
    Process32FirstW, Process32NextW, TH32CS_SNAPMODULE, TH32CS_SNAPMODULE32, TH32CS_SNAPPROCESS,
    TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use ::windows::Win32::System::Memory::{
    MEM_COMMIT, MEM_IMAGE, MEM_MAPPED, MEMORY_BASIC_INFORMATION, VirtualQueryEx,
};
use ::windows::Win32::System::ProcessStatus::GetMappedFileNameW;
use ::windows::Win32::System::RestartManager::{
    CCH_RM_SESSION_KEY, RM_PROCESS_INFO, RmEndSession, RmGetList, RmRegisterResources,
    RmStartSession,
};
use ::windows::Win32::System::Threading::{
    GetThreadTimes, OpenThread, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
    THREAD_QUERY_LIMITED_INFORMATION,
};
use ::windows::core::{PCWSTR, PWSTR};

use super::connections::owned_sockets;
use super::handle;
use crate::errors::{AppError, AppResult};
use crate::inspect::{
    MAX_REGIONS, MAX_SOCKETS, RawThread, Region, build_memory_map, strip_verbatim_prefix,
    windows_protection,
};
use crate::models::{
    FileUsage, FileUse, FileUserList, MemoryMap, MemoryRegionKind, ModuleInfo, OpenFileList,
    SocketEntry, SocketList,
};

/// A kernel handle closed on drop.
struct Owned(HANDLE);

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: the handle was returned by an API that transfers ownership.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn wide(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

// ----------------------------------------------------------------- sockets

fn process_names() -> HashMap<u32, String> {
    let mut out = HashMap::new();
    // SAFETY: plain FFI; the snapshot handle is closed by `Owned`.
    let Ok(snapshot) = (unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }) else {
        return out;
    };
    let snapshot = Owned(snapshot);
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    // SAFETY: `entry.dwSize` is set as the API requires.
    let mut ok = unsafe { Process32FirstW(snapshot.0, &mut entry) }.is_ok();
    while ok {
        out.insert(entry.th32ProcessID, wide(&entry.szExeFile));
        // SAFETY: as above.
        ok = unsafe { Process32NextW(snapshot.0, &mut entry) }.is_ok();
    }
    out
}

pub fn sockets() -> AppResult<SocketList> {
    let owned = owned_sockets()
        .ok_or_else(|| AppError::SystemQueryFailed("cannot read the TCP/UDP tables".into()))?;
    let names = process_names();
    let truncated = owned.len() > MAX_SOCKETS;
    let sockets = owned
        .into_iter()
        .take(MAX_SOCKETS)
        .map(|(pid, connection)| {
            // PID 0 means the system attributes the socket to no process.
            let pid = (pid != 0).then_some(pid);
            SocketEntry {
                connection,
                pid,
                process_name: pid.and_then(|p| names.get(&p).cloned()),
            }
        })
        .collect();
    Ok(SocketList {
        sockets,
        inaccessible_processes: 0,
        truncated,
    })
}

// ------------------------------------------------------------ files in use

pub fn open_files(_pid: u32) -> AppResult<OpenFileList> {
    Err(AppError::UnsupportedOperation(
        "Listing a process's open handles".into(),
    ))
}

struct RmSession(u32);

impl Drop for RmSession {
    fn drop(&mut self) {
        // SAFETY: the session was started by RmStartSession and ends once.
        let _ = unsafe { RmEndSession(self.0) };
    }
}

fn rm_error(what: &str, code: WIN32_ERROR) -> AppError {
    AppError::SystemQueryFailed(format!("Restart Manager {what} failed (error {})", code.0))
}

/// Applications and services holding `path` open, via Restart Manager.
/// <https://learn.microsoft.com/windows/win32/rstmgr/restart-manager-portal>
pub fn find_file_users(path: &str) -> AppResult<FileUserList> {
    let resolved = std::fs::canonicalize(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => {
            AppError::InvalidArgument("That path does not exist.".into())
        }
        std::io::ErrorKind::PermissionDenied => {
            AppError::PermissionDenied("You do not have permission to read that path.".into())
        }
        _ => AppError::SystemQueryFailed(format!("cannot resolve path: {e}")),
    })?;
    let display = strip_verbatim_prefix(&resolved.to_string_lossy());
    let is_directory = resolved.is_dir();
    let mut list = FileUserList {
        resolved_path: display.clone(),
        is_directory,
        uses: Vec::new(),
        inaccessible_processes: 0,
        truncated: false,
        note: Some(
            "Windows reports applications and services that hold this file. \
             Handles held by the system itself may not appear."
                .into(),
        ),
    };
    if is_directory {
        list.note = Some(
            "Windows can only report which programs use individual files, not folders. \
             Pick a file inside the folder instead."
                .into(),
        );
        return Ok(list);
    }

    let mut session = 0u32;
    let mut key = [0u16; CCH_RM_SESSION_KEY as usize + 1];
    // SAFETY: `key` holds CCH_RM_SESSION_KEY + 1 UTF-16 units as documented.
    let rc = unsafe { RmStartSession(&mut session, None, PWSTR(key.as_mut_ptr())) };
    if rc != NO_ERROR {
        return Err(rm_error("start", rc));
    }
    let session = RmSession(session);

    let path_w: Vec<u16> = display.encode_utf16().chain(std::iter::once(0)).collect();
    let names = [PCWSTR(path_w.as_ptr())];
    // SAFETY: `names` points at a NUL-terminated string that outlives the call.
    let rc = unsafe { RmRegisterResources(session.0, Some(&names), None, None) };
    if rc != NO_ERROR {
        return Err(rm_error("register", rc));
    }

    let mut infos: Vec<RM_PROCESS_INFO> = Vec::new();
    let (mut needed, mut count, mut reasons) = (0u32, 0u32, 0u32);
    for _ in 0..5 {
        let buffer = (!infos.is_empty()).then_some(infos.as_mut_ptr());
        // SAFETY: when `buffer` is set it addresses `count` initialised entries.
        let rc = unsafe { RmGetList(session.0, &mut needed, &mut count, buffer, &mut reasons) };
        if rc == NO_ERROR {
            infos.truncate(count as usize);
            break;
        }
        if rc == ERROR_MORE_DATA {
            // The set can grow between calls; leave headroom.
            infos.resize(needed as usize + 4, RM_PROCESS_INFO::default());
            count = u32::try_from(infos.len()).unwrap_or(u32::MAX);
            continue;
        }
        return Err(rm_error("list", rc));
    }

    list.uses = infos
        .iter()
        .map(|info| {
            let name = wide(&info.strAppName);
            FileUse {
                pid: info.Process.dwProcessId,
                process_name: if name.is_empty() {
                    wide(&info.strServiceShortName)
                } else {
                    name
                },
                usage: FileUsage::InUse,
                path: display.clone(),
            }
        })
        .collect();
    Ok(list)
}

// ------------------------------------------------------ modules and memory

fn module_error(pid: u32, e: &::windows::core::Error) -> AppError {
    // ToolHelp reports "cannot read that process's module list" this way.
    if e.code() == ERROR_PARTIAL_COPY.to_hresult() {
        AppError::PermissionDenied(
            "Windows would not let this app read that process's modules. \
             Running as administrator may allow it."
                .into(),
        )
    } else {
        handle::map_error(pid, e, "list the process's modules")
    }
}

struct Module {
    info: ModuleInfo,
    base: usize,
}

fn module_entries(pid: u32) -> AppResult<Vec<Module>> {
    // SAFETY: plain FFI; the snapshot handle is closed by `Owned`.
    let snapshot =
        unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, pid) }
            .map_err(|e| module_error(pid, &e))?;
    let snapshot = Owned(snapshot);
    let mut entry = MODULEENTRY32W {
        dwSize: size_of::<MODULEENTRY32W>() as u32,
        ..Default::default()
    };
    let mut out = Vec::new();
    // SAFETY: `entry.dwSize` is set as the API requires.
    let mut ok = unsafe { Module32FirstW(snapshot.0, &mut entry) }.is_ok();
    while ok {
        let base = entry.modBaseAddr as usize;
        out.push(Module {
            info: ModuleInfo {
                name: wide(&entry.szModule),
                path: wide(&entry.szExePath),
                base: format!("{base:x}"),
                size: u64::from(entry.modBaseSize),
                executable: true,
            },
            base,
        });
        // SAFETY: as above.
        ok = unsafe { Module32NextW(snapshot.0, &mut entry) }.is_ok();
    }
    Ok(out)
}

pub fn modules(pid: u32) -> AppResult<Vec<ModuleInfo>> {
    Ok(module_entries(pid)?.into_iter().map(|m| m.info).collect())
}

/// Committed regions of the address space. File paths come from
/// `GetMappedFileNameW` and are NT device paths (`\Device\HarddiskVolume3\...`),
/// shown as the system reports them. Heap and stack are not distinguishable
/// through this API, so private memory is reported as anonymous.
pub fn memory_map(pid: u32) -> AppResult<MemoryMap> {
    let process = handle::open(pid, PROCESS_QUERY_INFORMATION | PROCESS_VM_READ)?;
    // The first module is the executable itself.
    let main_base = module_entries(pid)
        .ok()
        .and_then(|m| m.first().map(|m| m.base));

    let mut regions: Vec<Region> = Vec::new();
    let mut address = 0usize;
    // (allocation base, resolved path): regions of one allocation share a file.
    let mut last_alloc: (usize, Option<String>) = (usize::MAX, None);
    loop {
        let mut mbi = MEMORY_BASIC_INFORMATION::default();
        // SAFETY: `mbi` is a valid out-parameter of the stated size.
        let got = unsafe {
            VirtualQueryEx(
                process.0,
                Some(address as *const c_void),
                &mut mbi,
                size_of::<MEMORY_BASIC_INFORMATION>(),
            )
        };
        if got == 0 {
            break;
        }
        let base = mbi.BaseAddress as usize;
        let size = mbi.RegionSize;
        if mbi.State == MEM_COMMIT {
            let alloc = mbi.AllocationBase as usize;
            let file_backed = mbi.Type == MEM_IMAGE || mbi.Type == MEM_MAPPED;
            if file_backed && last_alloc.0 != alloc {
                let mut buf = [0u16; 520];
                // SAFETY: valid process handle; `buf` is writable for its length.
                let n = unsafe { GetMappedFileNameW(process.0, base as *const c_void, &mut buf) };
                last_alloc = (alloc, (n > 0).then(|| wide(&buf[..n as usize])));
            }
            let path = file_backed.then(|| last_alloc.1.clone()).flatten();
            let kind = if mbi.Type == MEM_IMAGE {
                if Some(alloc) == main_base {
                    MemoryRegionKind::Image
                } else {
                    MemoryRegionKind::SharedLibrary
                }
            } else if mbi.Type == MEM_MAPPED {
                MemoryRegionKind::MappedFile
            } else {
                MemoryRegionKind::Anonymous
            };
            regions.push(Region {
                start: base as u64,
                end: (base as u64).saturating_add(size as u64),
                perms: windows_protection(mbi.Protect.0),
                offset: 0,
                path,
                kind,
                shared: mbi.Type == MEM_MAPPED,
            });
            if regions.len() > MAX_REGIONS {
                break;
            }
        }
        match base.checked_add(size) {
            Some(next) if next > address => address = next,
            _ => break,
        }
    }
    Ok(build_memory_map(regions))
}

// ----------------------------------------------------------------- threads

fn filetime(ft: FILETIME) -> u64 {
    (u64::from(ft.dwHighDateTime) << 32) | u64::from(ft.dwLowDateTime)
}

/// Seconds between 1601-01-01 (FILETIME epoch) and 1970-01-01.
const FILETIME_TO_UNIX_SECS: u64 = 11_644_473_600;

/// Threads of `pid`. State and last processor are not available through
/// documented, handle-light APIs, so they stay `None`; the UI shows a dash.
pub fn threads(pid: u32) -> AppResult<Vec<RawThread>> {
    // SAFETY: plain FFI; the snapshot handle is closed by `Owned`.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) }
        .map_err(|e| handle::map_error(pid, &e, "list threads"))?;
    let snapshot = Owned(snapshot);
    let mut entry = THREADENTRY32 {
        dwSize: size_of::<THREADENTRY32>() as u32,
        ..Default::default()
    };
    let mut out = Vec::new();
    // SAFETY: `entry.dwSize` is set as the API requires.
    let mut ok = unsafe { Thread32First(snapshot.0, &mut entry) }.is_ok();
    while ok {
        if entry.th32OwnerProcessID == pid {
            let (cpu_time_ms, start_time) = thread_times(entry.th32ThreadID);
            out.push(RawThread {
                tid: entry.th32ThreadID,
                name: None,
                state: None,
                cpu_time_ms,
                priority: Some(entry.tpBasePri + entry.tpDeltaPri),
                start_time,
                processor: None,
                wait_channel: None,
            });
        }
        // SAFETY: as above.
        ok = unsafe { Thread32Next(snapshot.0, &mut entry) }.is_ok();
    }
    if out.is_empty() {
        return Err(AppError::ProcessNotFound(pid));
    }
    Ok(out)
}

/// CPU time in ms and creation time (unix seconds); zero/`None` when the
/// thread cannot be opened (protected process) or has exited.
fn thread_times(tid: u32) -> (u64, Option<u64>) {
    // SAFETY: plain FFI; the handle is closed by `Owned`.
    let Ok(thread) = (unsafe { OpenThread(THREAD_QUERY_LIMITED_INFORMATION, false, tid) }) else {
        return (0, None);
    };
    let thread = Owned(thread);
    let (mut created, mut exited, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    // SAFETY: valid handle and out-parameters.
    if unsafe { GetThreadTimes(thread.0, &mut created, &mut exited, &mut kernel, &mut user) }
        .is_err()
    {
        return (0, None);
    }
    // FILETIME units are 100 ns.
    let cpu_ms = (filetime(kernel) + filetime(user)) / 10_000;
    let start = (filetime(created) / 10_000_000).checked_sub(FILETIME_TO_UNIX_SECS);
    (cpu_ms, start)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn own_listener_and_modules_are_visible() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let list = sockets().unwrap();
        assert!(
            list.sockets
                .iter()
                .any(|s| s.connection.local_port == port && s.pid == Some(std::process::id()))
        );
        let modules = modules(std::process::id()).unwrap();
        assert!(
            modules
                .iter()
                .any(|m| m.name.eq_ignore_ascii_case("kernel32.dll"))
        );
    }

    #[test]
    fn own_memory_map_and_threads() {
        let map = memory_map(std::process::id()).unwrap();
        assert!(map.total_bytes > 0 && map.executable_bytes > 0);
        let threads = threads(std::process::id()).unwrap();
        assert!(!threads.is_empty());
    }

    #[test]
    fn restart_manager_finds_a_file_this_process_holds() {
        let path = std::env::temp_dir().join(format!("tm-rm-{}.txt", std::process::id()));
        let file = std::fs::File::create(&path).unwrap();
        let list = find_file_users(path.to_str().unwrap()).unwrap();
        assert!(list.uses.iter().any(|u| u.pid == std::process::id()));
        drop(file);
        std::fs::remove_file(&path).unwrap();
    }
}
