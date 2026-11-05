//! Process control with documented Win32 APIs. No shell commands.

use ::windows::Win32::Foundation::{CloseHandle, FILETIME, HANDLE, HWND, LPARAM, WPARAM};
use ::windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use ::windows::Win32::System::Threading::{
    ABOVE_NORMAL_PRIORITY_CLASS, BELOW_NORMAL_PRIORITY_CLASS, GetProcessIdOfThread,
    GetProcessTimes, HIGH_PRIORITY_CLASS, IDLE_PRIORITY_CLASS, IsProcessCritical,
    NORMAL_PRIORITY_CLASS, OpenThread, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_INFORMATION,
    PROCESS_TERMINATE, ResumeThread, SetPriorityClass, SuspendThread,
    THREAD_QUERY_LIMITED_INFORMATION, THREAD_SUSPEND_RESUME, TerminateProcess,
};
use ::windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GetWindow, GetWindowThreadProcessId, IsWindowVisible, PostMessageW,
    WM_CLOSE,
};
use ::windows::core::BOOL;

use super::handle::{ProcessHandle, map_error, open};
use crate::errors::{AppError, AppResult};
use crate::models::{PriorityLevel, ProcessAction};

/// Validate creation time on the same handle used for destructive operations.
pub fn perform_action(pid: u32, start_time: u64, action: ProcessAction) -> AppResult<()> {
    let access = PROCESS_QUERY_LIMITED_INFORMATION
        | match action {
            ProcessAction::ForceKill => PROCESS_TERMINATE,
            ProcessAction::SetPriority { .. } => PROCESS_SET_INFORMATION,
            _ => PROCESS_QUERY_LIMITED_INFORMATION,
        };
    let handle = open(pid, access)?;
    let (mut created, mut exited, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    // SAFETY: owned process handle and correctly sized out-parameters.
    unsafe { GetProcessTimes(handle.0, &mut created, &mut exited, &mut kernel, &mut user) }
        .map_err(|e| map_error(pid, &e, "verify the process lifetime"))?;
    let ticks = (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime);
    let unix = (ticks / 10_000_000).checked_sub(11_644_473_600);
    if unix != Some(start_time) {
        return Err(AppError::ProcessNotFound(pid));
    }
    if action.is_disruptive() {
        let mut critical = BOOL::default();
        // SAFETY: owned process handle and valid out-parameter.
        unsafe { IsProcessCritical(handle.0, &mut critical) }
            .map_err(|e| map_error(pid, &e, "verify process safety"))?;
        if critical.as_bool() {
            return Err(AppError::ProcessProtected(
                "Windows marks this process as critical.".into(),
            ));
        }
    }
    match action {
        ProcessAction::Terminate => terminate(pid),
        ProcessAction::ForceKill => force_kill(pid, &handle),
        ProcessAction::Suspend => suspend(pid),
        ProcessAction::Resume => resume(pid),
        ProcessAction::SetPriority { priority } => set_priority(pid, &handle, priority),
    }
}

/// Graceful end: ask the process's visible top-level windows to close, as
/// clicking their X button would. The application may prompt to save.
/// Windowless processes cannot be asked this way.
/// <https://learn.microsoft.com/windows/win32/winmsg/wm-close>
fn terminate(pid: u32) -> AppResult<()> {
    let windows = top_level_windows(pid);
    if windows.is_empty() {
        return Err(AppError::ActionUnavailable(
            "This process has no window that can be asked to close. Use Force end to stop it immediately.".into(),
        ));
    }
    for hwnd in windows {
        let mut owner = 0;
        // SAFETY: a valid out-parameter; a vanished/reused HWND is rejected.
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut owner)) };
        if owner != pid {
            return Err(AppError::ProcessNotFound(pid));
        }
        // SAFETY: posting a message to a window handle is always memory safe.
        unsafe { PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) }
            .map_err(|e| map_error(pid, &e, "send a close request"))?;
    }
    Ok(())
}

/// <https://learn.microsoft.com/windows/win32/api/processthreadsapi/nf-processthreadsapi-terminateprocess>
fn force_kill(pid: u32, handle: &ProcessHandle) -> AppResult<()> {
    // SAFETY: valid handle with PROCESS_TERMINATE access.
    unsafe { TerminateProcess(handle.0, 1) }.map_err(|e| map_error(pid, &e, "end the process"))
}

fn suspend(pid: u32) -> AppResult<()> {
    let mut done = Vec::new();
    for tid in thread_ids(pid)? {
        let result = open_owned_thread(pid, tid).and_then(|thread| {
            // SAFETY: thread is owned, with suspend/resume access and verified owner.
            let previous = unsafe { SuspendThread(thread.0) };
            if previous == u32::MAX {
                return Err(::windows::core::Error::from_thread());
            }
            Ok(thread)
        });
        match result {
            Ok(thread) => done.push(thread),
            Err(e) => {
                for thread in &done {
                    // SAFETY: retained handles prevent TID reuse during rollback.
                    let _ = unsafe { ResumeThread(thread.0) };
                }
                return Err(thread_error(pid, e, "suspend"));
            }
        }
    }
    Ok(())
}

fn resume(pid: u32) -> AppResult<()> {
    for tid in thread_ids(pid)? {
        let thread = open_owned_thread(pid, tid).map_err(|e| thread_error(pid, e, "resume"))?;
        // SAFETY: live thread handle with verified owner and appropriate access.
        if unsafe { ResumeThread(thread.0) } == u32::MAX {
            return Err(thread_error(
                pid,
                ::windows::core::Error::from_thread(),
                "resume",
            ));
        }
    }
    Ok(())
}

/// <https://learn.microsoft.com/windows/win32/api/processthreadsapi/nf-processthreadsapi-setpriorityclass>
fn set_priority(pid: u32, handle: &ProcessHandle, level: PriorityLevel) -> AppResult<()> {
    let class = match level {
        PriorityLevel::Idle => IDLE_PRIORITY_CLASS,
        PriorityLevel::BelowNormal => BELOW_NORMAL_PRIORITY_CLASS,
        PriorityLevel::Normal => NORMAL_PRIORITY_CLASS,
        PriorityLevel::AboveNormal => ABOVE_NORMAL_PRIORITY_CLASS,
        PriorityLevel::High => HIGH_PRIORITY_CLASS,
    };
    // SAFETY: valid handle with PROCESS_SET_INFORMATION access.
    unsafe { SetPriorityClass(handle.0, class) }
        .map_err(|e| map_error(pid, &e, "change the priority"))
}

/// <https://learn.microsoft.com/windows/win32/api/processthreadsapi/nf-processthreadsapi-isprocesscritical>
pub fn is_critical(pid: u32) -> Option<bool> {
    let handle = open(pid, PROCESS_QUERY_LIMITED_INFORMATION).ok()?;
    let mut critical = BOOL::default();
    // SAFETY: valid handle and out-parameter.
    unsafe { IsProcessCritical(handle.0, &mut critical) }.ok()?;
    Some(critical.as_bool())
}

pub fn system_root() -> Option<String> {
    std::env::var("SystemRoot").ok()
}

// ------------------------------------------------------------------ helpers

struct EnumState {
    pid: u32,
    found: Vec<HWND>,
}

unsafe extern "system" fn collect_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: lparam is the &mut EnumState passed to EnumWindows below.
    let state = unsafe { &mut *(lparam.0 as *mut EnumState) };
    let mut owner_pid = 0u32;
    // SAFETY: valid out-parameter.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut owner_pid)) };
    if owner_pid == state.pid
        // SAFETY: plain queries on a window handle supplied by EnumWindows.
        && unsafe { IsWindowVisible(hwnd) }.as_bool()
        && unsafe { GetWindow(hwnd, GW_OWNER) }.is_err()
    {
        state.found.push(hwnd);
    }
    BOOL::from(true) // continue enumeration
}

fn top_level_windows(pid: u32) -> Vec<HWND> {
    let mut state = EnumState {
        pid,
        found: Vec::new(),
    };
    // SAFETY: the callback only touches `state`, which outlives the call.
    let _ = unsafe { EnumWindows(Some(collect_window), LPARAM(&mut state as *mut _ as isize)) };
    state.found
}

struct ThreadSnapshot(HANDLE);

impl Drop for ThreadSnapshot {
    fn drop(&mut self) {
        // SAFETY: handle from CreateToolhelp32Snapshot, closed once.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn thread_ids(pid: u32) -> AppResult<Vec<u32>> {
    // SAFETY: plain FFI; the handle is owned by ThreadSnapshot.
    let snap = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) }
        .map(ThreadSnapshot)
        .map_err(|e| map_error(pid, &e, "list the process's threads"))?;
    let mut entry = THREADENTRY32 {
        dwSize: size_of::<THREADENTRY32>() as u32,
        ..Default::default()
    };
    let mut ids = Vec::new();
    // SAFETY: correctly sized out-parameter.
    let mut ok = unsafe { Thread32First(snap.0, &mut entry) }.is_ok();
    while ok {
        if entry.th32OwnerProcessID == pid {
            ids.push(entry.th32ThreadID);
        }
        // SAFETY: as above.
        ok = unsafe { Thread32Next(snap.0, &mut entry) }.is_ok();
    }
    if ids.is_empty() {
        return Err(AppError::ProcessNotFound(pid));
    }
    Ok(ids)
}

fn open_owned_thread(pid: u32, tid: u32) -> Result<ProcessHandle, ::windows::core::Error> {
    // SAFETY: plain FFI; ProcessHandle owns and closes the returned kernel handle.
    let thread = ProcessHandle(unsafe {
        OpenThread(
            THREAD_SUSPEND_RESUME | THREAD_QUERY_LIMITED_INFORMATION,
            false,
            tid,
        )
    }?);
    // SAFETY: thread is a valid handle with query access. Reject reused thread IDs.
    if unsafe { GetProcessIdOfThread(thread.0) } != pid {
        return Err(::windows::core::Error::from_thread());
    }
    Ok(thread)
}

fn thread_error(pid: u32, e: ::windows::core::Error, verb: &str) -> AppError {
    map_error(pid, &e, &format!("{verb} the process"))
}
