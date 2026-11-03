//! Process control with signals and setpriority(2). No shell commands.

use crate::errors::{AppError, AppResult};
use crate::models::PriorityLevel;

pub fn terminate(pid: u32) -> AppResult<()> {
    signal(pid, libc::SIGTERM)
}

pub fn force_kill(pid: u32) -> AppResult<()> {
    signal(pid, libc::SIGKILL)
}

pub fn suspend(pid: u32) -> AppResult<()> {
    signal(pid, libc::SIGSTOP)
}

pub fn resume(pid: u32) -> AppResult<()> {
    signal(pid, libc::SIGCONT)
}

/// Raising priority (negative nice) requires CAP_SYS_NICE; the kernel's
/// EACCES/EPERM becomes a clear PermissionDenied.
pub fn set_priority(pid: u32, level: PriorityLevel) -> AppResult<()> {
    let nice = match level {
        PriorityLevel::Idle => 19,
        PriorityLevel::BelowNormal => 10,
        PriorityLevel::Normal => 0,
        PriorityLevel::AboveNormal => -5,
        PriorityLevel::High => -10,
    };
    // SAFETY: setpriority has no memory-safety preconditions.
    let rc = unsafe { libc::setpriority(libc::PRIO_PROCESS, pid as libc::id_t, nice) };
    if rc == 0 {
        Ok(())
    } else {
        Err(errno_error(pid, "change the priority of"))
    }
}

pub fn is_critical(_pid: u32) -> Option<bool> {
    None
}

pub fn system_root() -> Option<String> {
    None
}

fn signal(pid: u32, sig: libc::c_int) -> AppResult<()> {
    let target = libc::pid_t::try_from(pid).map_err(|_| AppError::InvalidPid(i64::from(pid)))?;
    // SAFETY: kill has no memory-safety preconditions. PIDs are validated as
    // positive, so this never signals a process group or every process.
    let rc = unsafe { libc::kill(target, sig) };
    if rc == 0 {
        Ok(())
    } else {
        Err(errno_error(pid, "signal"))
    }
}

fn errno_error(pid: u32, action: &str) -> AppError {
    let err = std::io::Error::last_os_error();
    match err.raw_os_error() {
        Some(libc::ESRCH) => AppError::ProcessNotFound(pid),
        Some(libc::EPERM | libc::EACCES) => AppError::PermissionDenied(format!(
            "Permission denied: the system does not allow this account to {action} this process. It usually belongs to another user or to the system."
        )),
        _ => AppError::SystemQueryFailed(format!("unable to {action} the process: {err}")),
    }
}
