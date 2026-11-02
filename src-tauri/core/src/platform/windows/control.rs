//! Process control with documented Win32 APIs. No shell commands.

use ::windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, WPARAM};
use ::windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use ::windows::Win32::System::Threading::{
    ABOVE_NORMAL_PRIORITY_CLASS, BELOW_NORMAL_PRIORITY_CLASS, HIGH_PRIORITY_CLASS,
    IDLE_PRIORITY_CLASS, IsProcessCritical, NORMAL_PRIORITY_CLASS, OpenThread,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_INFORMATION, PROCESS_TERMINATE, ResumeThread,
    SetPriorityClass, SuspendThread, THREAD_SUSPEND_RESUME, TerminateProcess,
};
use ::windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GetWindow, GetWindowThreadProcessId, IsWindowVisible, PostMessageW,
    WM_CLOSE,
};
use ::windows::core::BOOL;

use super::handle::{map_error, open};
use crate::errors::{AppError, AppResult};
use crate::models::PriorityLevel;

/// Graceful end: ask the process's visible top-level windows to close, as
/// clicking their X button would. The application may prompt to save.
/// Windowless processes cannot be asked this way.
/// <https://learn.microsoft.com/windows/win32/winmsg/wm-close>
pub fn terminate(pid: u32) -> AppResult<()> {
    let windows = top_level_windows(pid);
    if windows.is_empty() {
        return Err(AppError::ActionUnavailable(
            "This process has no window that can be asked to close. Use Force end to stop it immediately.".into(),
        ));
    }
    for hwnd in windows {
        // SAFETY: posting a message to a window handle is always memory safe.
        unsafe { PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) }
            .map_err(|e| map_error(pid, &e, "send a close request"))?;
    }
    Ok(())
}

/// <https://learn.microsoft.com/windows/win32/api/processthreadsapi/nf-processthreadsapi-terminateprocess>
pub fn force_kill(pid: u32) -> AppResult<()> {
    let handle = open(pid, PROCESS_TERMINATE)?;
    // SAFETY: valid handle with PROCESS_TERMINATE access.
    unsafe { TerminateProcess(handle.0, 1) }.map_err(|e| map_error(pid, &e, "end the process"))
}

pub fn suspend(pid: u32) -> AppResult<()> {
    let threads = thread_ids(pid)?;
    let mut done: Vec<u32> = Vec::with_capacity(threads.len());
    for tid in threads {
        if let Err(e) = with_thread(tid, |h| unsafe { SuspendThread(h) }) {
            // Leave the process as we found it.
            for t in &done {
                let _ = with_thread(*t, |h| unsafe { ResumeThread(h) });
            }
            return Err(thread_error(pid, e, "suspend"));
        }
        done.push(tid);
    }
    Ok(())
}

pub fn resume(pid: u32) -> AppResult<()> {
    for tid in thread_ids(pid)? {
        with_thread(tid, |h| unsafe { ResumeThread(h) })
            .map_err(|e| thread_error(pid, e, "resume"))?;
    }
    Ok(())
}

/// <https://learn.microsoft.com/windows/win32/api/processthreadsapi/nf-processthreadsapi-setpriorityclass>
pub fn set_priority(pid: u32, level: PriorityLevel) -> AppResult<()> {
    let class = match level {
        PriorityLevel::Idle => IDLE_PRIORITY_CLASS,
        PriorityLevel::BelowNormal => BELOW_NORMAL_PRIORITY_CLASS,
        PriorityLevel::Normal => NORMAL_PRIORITY_CLASS,
        PriorityLevel::AboveNormal => ABOVE_NORMAL_PRIORITY_CLASS,
        PriorityLevel::High => HIGH_PRIORITY_CLASS,
    };
    let handle = open(pid, PROCESS_SET_INFORMATION)?;
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

/// Opens a thread for suspend/resume and runs `op`, which returns the
/// previous suspend count or `u32::MAX` on failure.
fn with_thread(tid: u32, op: impl FnOnce(HANDLE) -> u32) -> Result<(), ::windows::core::Error> {
    // SAFETY: plain FFI; the handle is closed below.
    let handle = unsafe { OpenThread(THREAD_SUSPEND_RESUME, false, tid) }?;
    let result = op(handle);
    // SAFETY: handle from OpenThread, closed once.
    let _ = unsafe { CloseHandle(handle) };
    if result == u32::MAX {
        return Err(::windows::core::Error::from_thread());
    }
    Ok(())
}

fn thread_error(pid: u32, e: ::windows::core::Error, verb: &str) -> AppError {
    map_error(pid, &e, &format!("{verb} the process"))
}
