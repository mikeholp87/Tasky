//! Process control with signals and setpriority(2). No shell commands.

use crate::errors::{AppError, AppResult};
use crate::models::{PriorityLevel, ProcessAction};
use rustix::process::{Pid, PidfdFlags, Signal, pidfd_open, pidfd_send_signal};

pub struct ProcessPin {
    pid: u32,
    fd: std::os::fd::OwnedFd,
}

pub fn pin_process(pid: u32) -> AppResult<ProcessPin> {
    let raw = i32::try_from(pid).map_err(|_| AppError::InvalidPid(i64::from(pid)))?;
    let target = Pid::from_raw(raw).ok_or(AppError::InvalidPid(i64::from(pid)))?;
    let fd = pidfd_open(target, PidfdFlags::empty()).map_err(|e| {
        if e == rustix::io::Errno::NOSYS {
            AppError::UnsupportedOperation(
                "Safe process actions require Linux kernel 5.3 or newer (pidfd support).".into(),
            )
        } else {
            native_error(pid, "open", std::io::Error::from(e))
        }
    })?;
    Ok(ProcessPin { pid, fd })
}

impl ProcessPin {
    pub fn perform(&self, action: ProcessAction) -> AppResult<()> {
        let signal = match action {
            ProcessAction::Terminate => Signal::TERM,
            ProcessAction::ForceKill => Signal::KILL,
            ProcessAction::Suspend => Signal::STOP,
            ProcessAction::Resume => Signal::CONT,
            ProcessAction::SetPriority { priority } => return set_priority(self.pid, priority),
        };
        pidfd_send_signal(&self.fd, signal)
            .map_err(|e| native_error(self.pid, "signal", std::io::Error::from(e)))
    }
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

fn errno_error(pid: u32, action: &str) -> AppError {
    native_error(pid, action, std::io::Error::last_os_error())
}

fn native_error(pid: u32, action: &str, err: std::io::Error) -> AppError {
    match err.raw_os_error() {
        Some(libc::ESRCH) => AppError::ProcessNotFound(pid),
        Some(libc::EPERM | libc::EACCES) => AppError::PermissionDenied(format!(
            "The operating system denied permission to {action} this process. It may belong to another user; running with root privileges may allow it."
        )),
        _ => AppError::SystemQueryFailed(format!("unable to {action} the process: {err}")),
    }
}
