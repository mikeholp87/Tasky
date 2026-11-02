use ::windows::Win32::Foundation::{
    CloseHandle, E_ACCESSDENIED, ERROR_INVALID_PARAMETER, HANDLE, WIN32_ERROR,
};
use ::windows::Win32::System::SystemInformation::{
    IMAGE_FILE_MACHINE, IMAGE_FILE_MACHINE_AMD64, IMAGE_FILE_MACHINE_ARM64,
    IMAGE_FILE_MACHINE_I386, IMAGE_FILE_MACHINE_UNKNOWN,
};
use ::windows::Win32::System::Threading::{
    IsWow64Process2, OpenProcess, PROCESS_ACCESS_RIGHTS, PROCESS_QUERY_LIMITED_INFORMATION,
};

use crate::errors::{AppError, AppResult};

/// A process handle closed on drop.
pub struct ProcessHandle(pub HANDLE);

impl Drop for ProcessHandle {
    fn drop(&mut self) {
        // SAFETY: the handle came from OpenProcess and is closed exactly once.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// Opens `pid` with the given access, mapping failures to app errors.
/// <https://learn.microsoft.com/windows/win32/api/processthreadsapi/nf-processthreadsapi-openprocess>
pub fn open(pid: u32, access: PROCESS_ACCESS_RIGHTS) -> AppResult<ProcessHandle> {
    // SAFETY: plain FFI call; ownership of the handle moves into ProcessHandle.
    unsafe { OpenProcess(access, false, pid) }
        .map(ProcessHandle)
        .map_err(|e| map_error(pid, &e, "open the process"))
}

/// Translates Win32 errors into user-facing categories.
pub fn map_error(pid: u32, e: &::windows::core::Error, action: &str) -> AppError {
    let code = e.code();
    if code == E_ACCESSDENIED {
        AppError::PermissionDenied(format!(
            "Windows denied permission to {action}. It may belong to another user or a protected service; running Task Manager as administrator may allow it."
        ))
    } else if code == WIN32_ERROR(ERROR_INVALID_PARAMETER.0).to_hresult() {
        // OpenProcess reports a PID that no longer exists as an invalid parameter.
        AppError::ProcessNotFound(pid)
    } else {
        AppError::SystemQueryFailed(format!("unable to {action}: {}", e.message()))
    }
}

/// Instruction set the process runs as ("x64", "x86", "arm64").
/// <https://learn.microsoft.com/windows/win32/api/wow64apiset/nf-wow64apiset-iswow64process2>
pub fn architecture(pid: u32) -> Option<String> {
    let handle = open(pid, PROCESS_QUERY_LIMITED_INFORMATION).ok()?;
    let mut process = IMAGE_FILE_MACHINE::default();
    let mut native = IMAGE_FILE_MACHINE::default();
    // SAFETY: valid handle and out-parameters.
    unsafe { IsWow64Process2(handle.0, &mut process, Some(&mut native)) }.ok()?;
    // UNKNOWN means "not running under WOW64", i.e. native.
    let machine = if process == IMAGE_FILE_MACHINE_UNKNOWN {
        native
    } else {
        process
    };
    Some(machine_name(machine).to_owned())
}

fn machine_name(m: IMAGE_FILE_MACHINE) -> &'static str {
    match m {
        IMAGE_FILE_MACHINE_AMD64 => "x64",
        IMAGE_FILE_MACHINE_I386 => "x86",
        IMAGE_FILE_MACHINE_ARM64 => "arm64",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_own_architecture() {
        let arch = architecture(std::process::id()).expect("own process is accessible");
        let expected = if cfg!(target_arch = "x86_64") {
            "x64"
        } else if cfg!(target_arch = "aarch64") {
            "arm64"
        } else {
            "x86"
        };
        assert_eq!(arch, expected);
    }

    #[test]
    fn nonexistent_pid_maps_to_not_found_or_denied() {
        // PIDs are multiples of 4 on Windows, so an odd PID never exists.
        let err = open(0x7FFF_FFFF, PROCESS_QUERY_LIMITED_INFORMATION)
            .err()
            .expect("cannot open");
        assert!(matches!(
            err,
            AppError::ProcessNotFound(_) | AppError::PermissionDenied(_)
        ));
    }
}
