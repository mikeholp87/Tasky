//! Windows Service Control Manager. No shell commands or implicit elevation.
use std::time::{Duration, Instant};

use windows::Win32::Foundation::E_ACCESSDENIED;
use windows::Win32::System::Services::*;
use windows::core::{PCWSTR, PWSTR};

use crate::errors::{AppError, AppResult};
use crate::models::{ServiceAction, ServiceInfo, ServiceScope};

static CONTROL: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct ServiceHandle(SC_HANDLE);
impl Drop for ServiceHandle {
    fn drop(&mut self) {
        // SAFETY: uniquely owned SCM handle, closed exactly once.
        let _ = unsafe { CloseServiceHandle(self.0) };
    }
}

fn error(e: windows::core::Error) -> AppError {
    if e.code() == E_ACCESSDENIED {
        AppError::PermissionDenied("Windows denied access to this service. Changing system services may require administrator rights.".into())
    } else {
        AppError::SystemQueryFailed(format!("Windows service operation: {}", e.message()))
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn manager() -> AppResult<ServiceHandle> {
    // SAFETY: null names select the local machine and default database.
    unsafe {
        OpenSCManagerW(
            PCWSTR::null(),
            PCWSTR::null(),
            SC_MANAGER_CONNECT | SC_MANAGER_ENUMERATE_SERVICE,
        )
    }
    .map(ServiceHandle)
    .map_err(error)
}

fn open(scm: &ServiceHandle, name: &str, access: u32) -> AppResult<ServiceHandle> {
    let name = wide(name);
    // SAFETY: name is terminated and lives through the call.
    unsafe { OpenServiceW(scm.0, PCWSTR(name.as_ptr()), access) }
        .map(ServiceHandle)
        .map_err(error)
}

fn system_scope(scope: ServiceScope) -> AppResult<()> {
    if scope != ServiceScope::System {
        return Err(AppError::UnsupportedOperation(
            "Windows services are managed in the System list".into(),
        ));
    }
    Ok(())
}

fn protected_name(name: &str) -> bool {
    // Core OS, authentication, security and session infrastructure. Drivers
    // are excluded entirely by SERVICE_WIN32 enumeration.
    matches!(
        name.to_ascii_lowercase().as_str(),
        "rpcss"
            | "rpceptmapper"
            | "dcomlaunch"
            | "plugplay"
            | "power"
            | "eventlog"
            | "samss"
            | "lsm"
            | "profsvc"
            | "gpsvc"
            | "schedule"
            | "wininit"
            | "winlogon"
            | "bfe"
            | "mpssvc"
            | "windefend"
            | "wscsvc"
            | "securityhealthservice"
            | "sense"
            | "keyiso"
            | "nsi"
            | "dhcp"
            | "dnscache"
    )
}

fn status(handle: &ServiceHandle) -> AppResult<SERVICE_STATUS_PROCESS> {
    let mut value = SERVICE_STATUS_PROCESS::default();
    let mut needed = 0;
    // SAFETY: writable, properly aligned struct with exactly the advertised size.
    unsafe {
        QueryServiceStatusEx(
            handle.0,
            SC_STATUS_PROCESS_INFO,
            Some(std::slice::from_raw_parts_mut(
                (&mut value as *mut SERVICE_STATUS_PROCESS).cast(),
                std::mem::size_of_val(&value),
            )),
            &mut needed,
        )
    }
    .map_err(error)?;
    Ok(value)
}

fn info(
    scm: &ServiceHandle,
    name: String,
    description: String,
    status: SERVICE_STATUS_PROCESS,
) -> ServiceInfo {
    let (active, sub) = match status.dwCurrentState {
        SERVICE_RUNNING => ("active", "running"),
        SERVICE_START_PENDING => ("activating", "starting"),
        SERVICE_STOP_PENDING => ("deactivating", "stopping"),
        SERVICE_PAUSED => ("active", "paused"),
        SERVICE_PAUSE_PENDING | SERVICE_CONTINUE_PENDING => ("reloading", "pending"),
        _ if status.dwWin32ExitCode != 0 => ("failed", "failed"),
        _ => ("inactive", "dead"),
    };
    let mut protected = protected_name(&name)
        || (status.dwProcessId != 0
            && super::control::is_critical(status.dwProcessId) == Some(true));
    let mut startup = None;
    if let Ok(handle) = open(scm, &name, SERVICE_QUERY_CONFIG) {
        // Windows caps service configuration at 8 KiB. usize storage provides
        // the alignment required by QUERY_SERVICE_CONFIGW.
        let mut buffer = vec![0usize; 8192 / std::mem::size_of::<usize>()];
        let mut needed = 0;
        // SAFETY: valid handle and aligned, writable buffer; pointers in the
        // resulting structure are never retained past this allocation.
        if unsafe {
            QueryServiceConfigW(
                handle.0,
                Some(buffer.as_mut_ptr().cast()),
                8192,
                &mut needed,
            )
        }
        .is_ok()
        {
            let config = unsafe { &*buffer.as_ptr().cast::<QUERY_SERVICE_CONFIGW>() };
            startup = Some(
                match config.dwStartType {
                    SERVICE_AUTO_START => "Automatic",
                    SERVICE_DEMAND_START => "Manual",
                    SERVICE_DISABLED => "Disabled",
                    _ => "System",
                }
                .into(),
            );
            protected |= matches!(
                config.dwErrorControl,
                SERVICE_ERROR_SEVERE | SERVICE_ERROR_CRITICAL
            );
        } else {
            protected = true;
        }
        let mut protection = SERVICE_LAUNCH_PROTECTED_INFO::default();
        // SAFETY: buffer points to the matching protection struct.
        let read = unsafe {
            QueryServiceConfig2W(
                handle.0,
                SERVICE_CONFIG_LAUNCH_PROTECTED,
                Some(std::slice::from_raw_parts_mut(
                    (&mut protection as *mut SERVICE_LAUNCH_PROTECTED_INFO).cast(),
                    std::mem::size_of_val(&protection),
                )),
                &mut needed,
            )
        };
        protected |= read.is_err() || protection.dwLaunchProtected != 0;
    } else {
        protected = true;
    }
    ServiceInfo {
        unit: name,
        description,
        active_state: active.into(),
        sub_state: sub.into(),
        startup,
        main_pid: (status.dwProcessId != 0).then_some(status.dwProcessId),
        protected,
    }
}

pub fn services(scope: ServiceScope) -> AppResult<Vec<ServiceInfo>> {
    system_scope(scope)?;
    let scm = manager()?;
    let mut result = Vec::new();
    let mut resume = 0;
    loop {
        // EnumServicesStatusExW has a documented maximum 256 KiB buffer.
        let mut buffer = vec![0usize; 262144 / std::mem::size_of::<usize>()];
        let mut needed = 0;
        let mut count = 0;
        // SAFETY: buffer is aligned for ENUM_SERVICE_STATUS_PROCESSW; strings
        // returned by SCM are terminated and remain inside this live buffer.
        let read = unsafe {
            EnumServicesStatusExW(
                scm.0,
                SC_ENUM_PROCESS_INFO,
                SERVICE_WIN32,
                SERVICE_STATE_ALL,
                Some(std::slice::from_raw_parts_mut(
                    buffer.as_mut_ptr().cast(),
                    262144,
                )),
                &mut needed,
                &mut count,
                Some(&mut resume),
                PCWSTR::null(),
            )
        };
        if let Err(e) = &read
            && e.code() != windows::Win32::Foundation::ERROR_MORE_DATA.to_hresult()
        {
            return Err(error(e.clone()));
        }
        let entries = unsafe {
            std::slice::from_raw_parts(
                buffer.as_ptr().cast::<ENUM_SERVICE_STATUS_PROCESSW>(),
                count as usize,
            )
        };
        for entry in entries {
            // SAFETY: SCM owns valid UTF-16 strings in the enumeration buffer.
            let string = |p: PWSTR| unsafe { p.to_string().unwrap_or_default() };
            result.push(info(
                &scm,
                string(entry.lpServiceName),
                string(entry.lpDisplayName),
                entry.ServiceStatusProcess,
            ));
        }
        if read.is_ok() {
            break;
        }
        if count == 0 {
            return Err(AppError::SystemQueryFailed(
                "Windows service enumeration made no progress.".into(),
            ));
        }
    }
    result.sort_by_key(|s| s.unit.to_ascii_lowercase());
    Ok(result)
}

pub fn control_service(
    scope: ServiceScope,
    unit: &str,
    action: ServiceAction,
    confirm: &dyn Fn(&ServiceInfo) -> AppResult<()>,
) -> AppResult<()> {
    system_scope(scope)?;
    let _guard = CONTROL.try_lock().map_err(|_| AppError::OperationBusy)?;
    if unit.is_empty()
        || unit.len() > 256
        || unit
            .chars()
            .any(|c| c.is_control() || c == '/' || c == '\\')
    {
        return Err(AppError::InvalidArgument(
            "Invalid Windows service name.".into(),
        ));
    }
    let scm = manager()?;
    // Retain one service handle through confirmation and control so a deleted
    // service name cannot be replaced with a different service mid-operation.
    let access = SERVICE_QUERY_STATUS
        | SERVICE_QUERY_CONFIG
        | match action {
            ServiceAction::Start => SERVICE_START,
            ServiceAction::Stop => SERVICE_STOP,
            ServiceAction::Restart => SERVICE_START | SERVICE_STOP,
        };
    let handle = open(&scm, unit, access)?;
    let current = status(&handle)?;
    let service = info(&scm, unit.into(), unit.into(), current);
    if service.protected {
        return Err(AppError::ProcessProtected(
            "This system-critical or protected service cannot be changed from Tasky.".into(),
        ));
    }
    confirm(&service)?;
    let current = status(&handle)?;
    if current.dwProcessId != 0 && super::control::is_critical(current.dwProcessId) == Some(true) {
        return Err(AppError::ProcessProtected(
            "This service now runs in a critical process.".into(),
        ));
    }
    if action != ServiceAction::Start && current.dwCurrentState != SERVICE_STOPPED {
        if !matches!(current.dwCurrentState, SERVICE_RUNNING | SERVICE_PAUSED)
            || current.dwControlsAccepted & SERVICE_ACCEPT_STOP == 0
        {
            return Err(AppError::ActionUnavailable(
                "This service cannot be stopped in its current state.".into(),
            ));
        }
        let mut stopped = SERVICE_STATUS::default();
        // SAFETY: live SCM service handle with SERVICE_STOP access.
        unsafe { ControlService(handle.0, SERVICE_CONTROL_STOP, &mut stopped) }.map_err(error)?;
        let deadline = Instant::now() + Duration::from_secs(30);
        while status(&handle)?.dwCurrentState != SERVICE_STOPPED {
            if Instant::now() >= deadline {
                return Err(AppError::ActionUnavailable(
                    "The service has not stopped yet. Refresh before trying again.".into(),
                ));
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }
    if action != ServiceAction::Stop {
        if status(&handle)?.dwCurrentState == SERVICE_RUNNING && action == ServiceAction::Start {
            return Ok(());
        }
        // SAFETY: the service is selected by a validated SCM handle, no arguments.
        unsafe { StartServiceW(handle.0, None) }.map_err(error)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_list_includes_protected_rpc_and_hides_drivers() {
        let items = services(ServiceScope::System).unwrap();
        assert!(
            items
                .iter()
                .any(|s| s.unit.eq_ignore_ascii_case("RpcSs") && s.protected)
        );
        assert!(!items.iter().any(|s| s.unit.eq_ignore_ascii_case("disk")));
    }
    #[test]
    fn protected_service_is_refused_before_confirmation() {
        let result = control_service(ServiceScope::System, "RpcSs", ServiceAction::Stop, &|_| {
            panic!("must never confirm a protected service")
        });
        assert!(matches!(
            result,
            Err(AppError::ProcessProtected(_)) | Err(AppError::PermissionDenied(_))
        ));
    }
}
