//! Process control with signals and setpriority(2). No shell commands.

use crate::errors::{AppError, AppResult};
use crate::models::PriorityLevel;

/// Open a lifetime-bound signal target before consulting the live process.
/// There is deliberately no numeric-PID fallback on kernels without pidfds.
pub fn perform_action(
    pid: u32,
    start_time: u64,
    action: crate::models::ProcessAction,
) -> AppResult<()> {
    use crate::models::ProcessAction;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use sysinfo::{ProcessRefreshKind, ProcessStatus};
    let target = libc::pid_t::try_from(pid)
        .ok()
        .filter(|p| *p > 0)
        .ok_or(AppError::InvalidPid(i64::from(pid)))?;
    // SAFETY: pidfd_open takes scalar arguments and returns a new owned fd.
    let raw = unsafe { libc::syscall(libc::SYS_pidfd_open, target, 0) };
    if raw < 0 {
        return Err(errno_error(pid, "open a stable process reference to"));
    }
    // SAFETY: a successful pidfd_open returns a new descriptor, owned once.
    let fd = unsafe { OwnedFd::from_raw_fd(raw as i32) };
    let stopped = crate::platform::with_process(
        pid,
        start_time,
        ProcessRefreshKind::nothing().without_tasks(),
        |p| matches!(p.status(), ProcessStatus::Stop | ProcessStatus::Tracing),
    )?;
    let sig = match action {
        ProcessAction::Terminate => libc::SIGTERM,
        ProcessAction::ForceKill => libc::SIGKILL,
        ProcessAction::Suspend => libc::SIGSTOP,
        ProcessAction::Resume => libc::SIGCONT,
        // shortcut: Linux has no pidfd priority API; the live check narrows,
        // but cannot eliminate, the setpriority PID race. See docs/archive/security-2026-11-03/SECURITY_AUDIT.md.
        ProcessAction::SetPriority { priority } => return set_priority(pid, priority),
    };
    let send = |sig: libc::c_int| {
        // SAFETY: fd is live; null siginfo and zero flags request an ordinary signal.
        let rc = unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                fd.as_raw_fd(),
                sig,
                std::ptr::null::<libc::siginfo_t>(),
                0,
            )
        };
        if rc == 0 {
            Ok(())
        } else {
            Err(errno_error(pid, "signal"))
        }
    };
    send(sig)?;
    // A stopped process only acts on SIGTERM once it is continued.
    if sig == libc::SIGTERM && stopped {
        send(libc::SIGCONT)?;
    }
    Ok(())
}

/// Raising priority (negative nice) requires CAP_SYS_NICE; the kernel's
/// EACCES/EPERM becomes a clear PermissionDenied.
fn set_priority(pid: u32, level: PriorityLevel) -> AppResult<()> {
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
