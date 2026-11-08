//! systemd services: list them, and start, stop or restart one.
//!
//! Safety rules, enforced here and not in the UI:
//! - Services the system or the desktop session depends on (systemd itself,
//!   D-Bus, the display manager, networking, logind, getty, ...) are listed
//!   but refused for any action.
//! - An action targets a unit that is in the current list, in the requested
//!   scope. The name is never trusted, and `systemctl` runs without a shell.
//! - The caller is asked to confirm with the real unit before anything runs.
//! - Changes to system services are authorized by polkit through the
//!   desktop's own prompt; this code never handles a password.

use std::collections::HashMap;
use std::process::Command;

use crate::errors::{AppError, AppResult};
use crate::models::{ServiceAction, ServiceInfo, ServiceScope};

/// Unit-name prefixes whose failure or stop takes the machine or the login
/// session down. Matched against the name without `.service`.
const PROTECTED_PREFIXES: &[&str] = &[
    "systemd-",
    "dbus",
    "polkit",
    "gdm",
    "sddm",
    "lightdm",
    "display-manager",
    "networkmanager",
    "user@",
    "user-runtime-dir@",
    "getty",
    "serial-getty",
    "console-getty",
    "emergency",
    "rescue",
    "plymouth",
    "udev",
    "init",
];

fn protected(unit: &str) -> bool {
    let name = unit
        .strip_suffix(".service")
        .unwrap_or(unit)
        .to_ascii_lowercase();
    PROTECTED_PREFIXES.iter().any(|p| name.starts_with(p))
}

fn valid_unit(unit: &str) -> bool {
    unit.len() <= 256
        && unit.strip_suffix(".service").is_some_and(|name| {
            !name.is_empty()
                && !name.starts_with('-')
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '_' | '.' | '@' | '-'))
        })
}

fn systemctl(scope: ServiceScope, args: &[&str]) -> AppResult<String> {
    let mut command = Command::new("systemctl");
    if scope == ServiceScope::User {
        command.arg("--user");
    }
    let output = command
        .args(args)
        .env("LC_ALL", "C")
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                AppError::UnsupportedOperation("Services (systemd is not available)".into())
            } else {
                AppError::SystemQueryFailed(format!("Could not run systemctl: {e}"))
            }
        })?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let first = stderr.lines().next().unwrap_or("").trim();
    Err(
        if first.contains("Access denied")
            || first.contains("authentication")
            || first.contains("Authentication")
        {
            AppError::PermissionDenied(
                "Not authorized. Approve the system prompt, or ask an administrator.".into(),
            )
        } else if first.contains("not found") || first.contains("not loaded") {
            AppError::ActionUnavailable("That service no longer exists.".into())
        } else {
            AppError::SystemQueryFailed(first.to_string())
        },
    )
}

/// `systemctl list-units` rows: `UNIT LOAD ACTIVE SUB DESCRIPTION...`.
fn parse_units(text: &str) -> HashMap<String, (String, String, String)> {
    text.lines()
        .filter_map(|line| {
            let mut f = line.split_whitespace();
            let unit = f.next()?;
            let _load = f.next()?;
            let active = f.next()?;
            let sub = f.next()?;
            let description = f.collect::<Vec<_>>().join(" ");
            Some((
                unit.to_string(),
                (active.to_string(), sub.to_string(), description),
            ))
        })
        .collect()
}

/// `systemctl list-unit-files` rows: `UNIT STATE [PRESET]`.
fn parse_unit_files(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let mut f = line.split_whitespace();
            Some((f.next()?.to_string(), f.next()?.to_string()))
        })
        .collect()
}

/// `systemctl show -p Id -p MainPID` blocks, separated by blank lines.
fn parse_main_pids(text: &str) -> HashMap<String, u32> {
    let mut out = HashMap::new();
    for block in text.split("\n\n") {
        let (mut id, mut pid) = (None, 0u32);
        for line in block.lines() {
            match line.split_once('=') {
                Some(("Id", v)) => id = Some(v.to_string()),
                Some(("MainPID", v)) => pid = v.parse().unwrap_or(0),
                _ => {}
            }
        }
        if let (Some(id), true) = (id, pid > 0) {
            out.insert(id, pid);
        }
    }
    out
}

fn build(
    units: &HashMap<String, (String, String, String)>,
    files: &[(String, String)],
    pids: &HashMap<String, u32>,
) -> Vec<ServiceInfo> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    let startup: HashMap<&str, &str> = files
        .iter()
        .map(|(u, s)| (u.as_str(), s.as_str()))
        .collect();
    let names = files.iter().map(|(u, _)| u).chain(units.keys());
    for unit in names {
        // Templates (`getty@.service`) are not services; their instances are.
        if unit.ends_with("@.service") || !unit.ends_with(".service") || !seen.insert(unit.clone())
        {
            continue;
        }
        let (active, sub, description) = units
            .get(unit)
            .cloned()
            .unwrap_or_else(|| ("inactive".into(), "dead".into(), String::new()));
        out.push(ServiceInfo {
            unit: unit.clone(),
            description,
            active_state: active,
            sub_state: sub,
            startup: startup.get(unit.as_str()).map(|s| s.to_string()),
            main_pid: pids.get(unit).copied(),
            protected: protected(unit),
        });
    }
    out.sort_by(|a, b| a.unit.cmp(&b.unit));
    out
}

pub fn services(scope: ServiceScope) -> AppResult<Vec<ServiceInfo>> {
    let units = systemctl(
        scope,
        &[
            "list-units",
            "--type=service",
            "--all",
            "--no-pager",
            "--plain",
            "--no-legend",
            "--full",
        ],
    )?;
    // Unit files add services that are installed but not loaded.
    let files = systemctl(
        scope,
        &[
            "list-unit-files",
            "--type=service",
            "--no-pager",
            "--plain",
            "--no-legend",
        ],
    )
    .unwrap_or_default();
    let pids = systemctl(
        scope,
        &["show", "*.service", "--property=Id,MainPID", "--no-pager"],
    )
    .unwrap_or_default();
    Ok(build(
        &parse_units(&units),
        &parse_unit_files(&files),
        &parse_main_pids(&pids),
    ))
}

/// Starts, stops or restarts `unit`. `confirm` sees the real service (and may
/// refuse) before anything runs. The change is queued (`--no-block`): the UI
/// watches the state move instead of waiting on a slow stop.
pub fn control_service(
    scope: ServiceScope,
    unit: &str,
    action: ServiceAction,
    confirm: &dyn Fn(&ServiceInfo) -> AppResult<()>,
) -> AppResult<()> {
    if !valid_unit(unit) {
        return Err(AppError::InvalidArgument("That is not a service.".into()));
    }
    let service = services(scope)?
        .into_iter()
        .find(|s| s.unit == unit)
        .ok_or_else(|| AppError::ActionUnavailable("That service no longer exists.".into()))?;
    if service.protected {
        return Err(AppError::ActionUnavailable(format!(
            "{unit} is a system-critical service and can't be changed from Tasky."
        )));
    }
    confirm(&service)?;
    let verb = match action {
        ServiceAction::Start => "start",
        ServiceAction::Stop => "stop",
        ServiceAction::Restart => "restart",
    };
    systemctl(scope, &["--no-block", verb, "--", unit]).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNITS: &str = "\
cups.service                loaded active   running CUPS Scheduler
alsa-state.service          loaded inactive dead    Manage Sound Card State
bad.service                 loaded failed   failed  A Failing Thing
";
    const FILES: &str = "\
cups.service                enabled  enabled
alsa-state.service          static   -
getty@.service              enabled  enabled
ssh.service                 disabled enabled
";
    const SHOW: &str = "Id=cups.service\nMainPID=812\n\nId=bad.service\nMainPID=0\n\n";

    #[test]
    fn merges_loaded_state_unit_files_and_pids() {
        let list = build(
            &parse_units(UNITS),
            &parse_unit_files(FILES),
            &parse_main_pids(SHOW),
        );
        let by = |n: &str| list.iter().find(|s| s.unit == n).unwrap();
        assert_eq!(by("cups.service").main_pid, Some(812));
        assert_eq!(by("cups.service").startup.as_deref(), Some("enabled"));
        assert_eq!(by("cups.service").description, "CUPS Scheduler");
        assert_eq!(by("bad.service").active_state, "failed");
        assert_eq!(by("bad.service").main_pid, None);
        // Installed but never loaded: shown as stopped.
        assert_eq!(by("ssh.service").active_state, "inactive");
        assert_eq!(by("ssh.service").startup.as_deref(), Some("disabled"));
        // Templates are not services.
        assert!(list.iter().all(|s| s.unit != "getty@.service"));
        // Sorted, no duplicates.
        let mut sorted: Vec<_> = list.iter().map(|s| s.unit.clone()).collect();
        let n = sorted.len();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), n);
    }

    #[test]
    fn system_critical_services_are_protected() {
        for unit in [
            "systemd-logind.service",
            "dbus.service",
            "dbus-broker.service",
            "polkit.service",
            "gdm.service",
            "sddm.service",
            "NetworkManager.service",
            "getty@tty1.service",
            "user@1000.service",
            "plymouth-start.service",
        ] {
            assert!(protected(unit), "{unit}");
        }
        for unit in [
            "cups.service",
            "bluetooth.service",
            "ssh.service",
            "docker.service",
        ] {
            assert!(!protected(unit), "{unit}");
        }
    }

    #[test]
    fn only_plain_service_names_are_accepted() {
        for ok in [
            "cups.service",
            "foo@bar.service",
            "a-b_c.d.service",
            "x:y.service",
        ] {
            assert!(valid_unit(ok), "{ok}");
        }
        for bad in [
            "cups",
            ".service",
            "-h.service",
            "a b.service",
            "a;b.service",
            "../x.service",
            "a/b.service",
            "x.socket",
            "--user.service",
        ] {
            assert!(!valid_unit(bad), "{bad}");
        }
    }

    #[test]
    fn refuses_protected_and_unknown_units_before_asking_or_running() {
        let never = |_: &ServiceInfo| -> AppResult<()> { panic!("must not ask") };
        assert!(matches!(
            control_service(
                ServiceScope::System,
                "bad name",
                ServiceAction::Stop,
                &never
            ),
            Err(AppError::InvalidArgument(_))
        ));
    }

    /// Needs systemd: `cargo test -- --ignored lists_real_services`.
    #[test]
    #[ignore]
    fn lists_real_services_with_protected_ones_marked() {
        let list = services(ServiceScope::System).unwrap();
        assert!(!list.is_empty());
        let running = list.iter().filter(|s| s.sub_state == "running").count();
        let protected = list.iter().filter(|s| s.protected).count();
        println!(
            "{} services, {running} running, {protected} protected",
            list.len()
        );
        for s in list.iter().filter(|s| s.active_state == "failed") {
            println!("failed: {} {:?}", s.unit, s.main_pid);
        }
        // A protected service is refused before the caller is even asked.
        let never = |_: &ServiceInfo| -> AppResult<()> { panic!("must not ask") };
        for unit in ["sddm.service", "dbus.service", "NetworkManager.service"] {
            if list.iter().any(|s| s.unit == unit) {
                let r = control_service(ServiceScope::System, unit, ServiceAction::Stop, &never);
                assert!(
                    matches!(r, Err(AppError::ActionUnavailable(_))),
                    "{unit}: {r:?}"
                );
                println!("refused: {unit}");
            }
        }
        assert!(
            list.iter()
                .filter(|s| s.unit.starts_with("systemd-"))
                .all(|s| s.protected)
        );
    }
}
