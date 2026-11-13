//! Safeguards against destabilizing the system. Pure rules over gathered
//! facts, so they are unit-tested on every platform.
//!
//! Names alone never *block*: a blocked verdict needs an OS signal (critical
//! flag, PID 0/1/4, kernel thread) or a verified System32 path. Name-only
//! matches produce warnings.

use crate::models::{
    Platform, PriorityLevel, ProcessAction, ProtectionAssessment, ProtectionLevel,
};

/// What we know about the target process.
#[derive(Debug, Clone, Default)]
pub struct ProcessFacts<'a> {
    pub pid: u32,
    pub parent_pid: Option<u32>,
    pub name: &'a str,
    pub exe: Option<&'a str>,
    pub user: Option<&'a str>,
    pub self_pid: u32,
    /// Windows `IsProcessCritical`; `None` when it could not be queried.
    pub is_critical: Option<bool>,
    /// Windows `%SystemRoot%`, e.g. `C:\Windows`.
    pub system_root: Option<&'a str>,
    /// Linux `PF_KTHREAD`.
    pub kernel_thread: bool,
    /// Already exited, waiting for its parent to reap it.
    pub zombie: bool,
}

/// Session/kernel processes whose loss crashes or locks Windows.
const WINDOWS_CRITICAL: &[&str] = &[
    "csrss.exe",
    "wininit.exe",
    "smss.exe",
    "services.exe",
    "lsass.exe",
    "winlogon.exe",
];

/// Core Linux system daemons; ending them can break login, networking or
/// the desktop session.
const LINUX_SYSTEM: &[&str] = &[
    "systemd",
    "systemd-journald",
    "systemd-logind",
    "systemd-udevd",
    "dbus-daemon",
    "dbus-broker",
    "NetworkManager",
    "polkitd",
    "sshd",
    "gdm",
    "gdm3",
    "sddm",
    "lightdm",
    "Xorg",
    "Xwayland",
    "gnome-shell",
    "plasmashell",
    "kwin_wayland",
    "kwin_x11",
];

pub fn assess(
    platform: Platform,
    facts: &ProcessFacts<'_>,
    action: ProcessAction,
) -> ProtectionAssessment {
    let mut a = ProtectionAssessment::allowed();
    let disruptive = action.is_disruptive();

    if facts.pid == facts.self_pid && disruptive {
        a.add(
            ProtectionLevel::Blocked,
            "This is Task Manager itself. Close the window or use Quit instead.",
        );
    } else if facts.parent_pid == Some(facts.self_pid)
        && facts.name.eq_ignore_ascii_case("msedgewebview2.exe")
        && disruptive
    {
        // Other children (e.g. a program opened from here) stay manageable.
        a.add(
            ProtectionLevel::Blocked,
            "This is part of this app: it draws the window or fetches for it. Quit the app instead.",
        );
    }

    match platform {
        Platform::Windows => assess_windows(facts, action, &mut a),
        Platform::Linux | Platform::Macos | Platform::Other => assess_unix(facts, action, &mut a),
    }

    if matches!(
        action,
        ProcessAction::SetPriority {
            priority: PriorityLevel::High
        }
    ) {
        a.add(
            ProtectionLevel::Warning,
            "High priority lets this process take CPU time from everything else, which can make the system less responsive.",
        );
    }
    a
}

fn assess_windows(facts: &ProcessFacts<'_>, action: ProcessAction, a: &mut ProtectionAssessment) {
    if facts.pid == 0 || facts.pid == 4 {
        a.add(
            ProtectionLevel::Blocked,
            "This is a core part of the Windows kernel and cannot be changed.",
        );
        return;
    }
    if !action.is_disruptive() {
        return;
    }
    if facts.is_critical == Some(true) {
        a.add(
            ProtectionLevel::Blocked,
            "Windows marks this process as critical. Ending or suspending it would crash the system.",
        );
    }

    let name = facts.name.to_ascii_lowercase();
    let in_system32 = match (facts.exe, facts.system_root) {
        (Some(exe), Some(root)) => {
            let expected = format!("{}\\system32\\{}", root.trim_end_matches('\\'), name);
            exe.eq_ignore_ascii_case(&expected)
        }
        _ => false,
    };

    if WINDOWS_CRITICAL.contains(&name.as_str()) {
        if in_system32 {
            a.add(
                ProtectionLevel::Blocked,
                "This is a critical Windows system process. Ending it would crash or lock the system.",
            );
        } else {
            a.add(
                ProtectionLevel::Warning,
                "This has the name of a critical Windows system process. Its location could not be verified.",
            );
        }
    }
    match name.as_str() {
        "svchost.exe" if in_system32 || facts.exe.is_none() => a.add(
            ProtectionLevel::Warning,
            "This hosts Windows services. Other programs and Windows features may stop working.",
        ),
        "explorer.exe" => a.add(
            ProtectionLevel::Warning,
            "This is the Windows shell. The taskbar, Start menu and desktop will disappear until it restarts.",
        ),
        "sihost.exe" | "logonui.exe" if in_system32 => a.add(
            ProtectionLevel::Warning,
            "This runs part of your sign-in session. Ending it may lock or sign you out and close open programs.",
        ),
        "dwm.exe" => a.add(
            ProtectionLevel::Warning,
            "This is the Desktop Window Manager. The screen may flicker or go black while Windows restarts it.",
        ),
        _ => {}
    }
}

fn assess_unix(facts: &ProcessFacts<'_>, action: ProcessAction, a: &mut ProtectionAssessment) {
    if !action.is_disruptive() {
        return;
    }
    if facts.pid == 1 {
        a.add(
            ProtectionLevel::Blocked,
            "PID 1 is the init system. Stopping it would bring down the whole system.",
        );
        return;
    }
    // kthreadd (PID 2) and its children are kernel threads; signals cannot
    // stop them and trying is meaningless.
    if facts.kernel_thread || facts.pid == 2 || facts.parent_pid == Some(2) {
        a.add(
            ProtectionLevel::Blocked,
            "This is a kernel thread and cannot be stopped from user space.",
        );
        return;
    }
    if facts.zombie {
        a.add(
            ProtectionLevel::Blocked,
            "This process has already exited. It is a zombie: only its entry in the process table remains, and it disappears once its parent process collects its exit status. Signals have no effect on it; ending or restarting the parent process removes it.",
        );
        return;
    }
    let is_root = facts.user == Some("root");
    let known_daemon = LINUX_SYSTEM.contains(&facts.name) || facts.name.starts_with("systemd-");
    if is_root && known_daemon {
        a.add(
            ProtectionLevel::Warning,
            "This is a core system service. Stopping it may break login, networking or the desktop session.",
        );
    } else if is_root && facts.parent_pid == Some(1) {
        a.add(
            ProtectionLevel::Warning,
            "This is a system service run by root. Other programs may depend on it.",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOT: &str = r"C:\Windows";

    fn win(name: &'static str, exe: Option<&'static str>) -> ProcessFacts<'static> {
        ProcessFacts {
            pid: 500,
            parent_pid: Some(400),
            name,
            exe,
            user: None,
            self_pid: 9_999,
            is_critical: Some(false),
            system_root: Some(ROOT),
            ..Default::default()
        }
    }

    fn level(
        platform: Platform,
        facts: &ProcessFacts<'_>,
        action: ProcessAction,
    ) -> ProtectionLevel {
        assess(platform, facts, action).level
    }

    #[test]
    fn never_ends_itself_but_can_change_own_priority() {
        let mut f = win("taskmanager.exe", None);
        f.pid = f.self_pid;
        assert_eq!(
            level(Platform::Windows, &f, ProcessAction::ForceKill),
            ProtectionLevel::Blocked
        );
        assert_eq!(
            level(Platform::Windows, &f, ProcessAction::Suspend),
            ProtectionLevel::Blocked
        );
        let lower = ProcessAction::SetPriority {
            priority: PriorityLevel::BelowNormal,
        };
        assert_eq!(
            level(Platform::Windows, &f, lower),
            ProtectionLevel::Allowed
        );
    }

    #[test]
    fn blocks_windows_kernel_pids_and_critical_flag() {
        let mut f = win("System", None);
        f.pid = 4;
        assert_eq!(
            level(Platform::Windows, &f, ProcessAction::Terminate),
            ProtectionLevel::Blocked
        );

        let mut c = win("anything.exe", Some(r"C:\Tools\anything.exe"));
        c.is_critical = Some(true);
        assert_eq!(
            level(Platform::Windows, &c, ProcessAction::ForceKill),
            ProtectionLevel::Blocked
        );
    }

    #[test]
    fn critical_names_block_only_when_path_is_verified() {
        let real = win("lsass.exe", Some(r"C:\Windows\System32\lsass.exe"));
        assert_eq!(
            level(Platform::Windows, &real, ProcessAction::Terminate),
            ProtectionLevel::Blocked
        );

        let impostor = win("lsass.exe", Some(r"C:\Users\x\Downloads\lsass.exe"));
        assert_eq!(
            level(Platform::Windows, &impostor, ProcessAction::Terminate),
            ProtectionLevel::Warning
        );
    }

    #[test]
    fn warns_for_shell_and_services() {
        let explorer = win("explorer.exe", Some(r"C:\Windows\explorer.exe"));
        assert_eq!(
            level(Platform::Windows, &explorer, ProcessAction::Terminate),
            ProtectionLevel::Warning
        );
        let svc = win("svchost.exe", Some(r"C:\Windows\System32\svchost.exe"));
        assert_eq!(
            level(Platform::Windows, &svc, ProcessAction::Suspend),
            ProtectionLevel::Warning
        );
        // Non-disruptive actions are not warned about.
        assert_eq!(
            level(Platform::Windows, &svc, ProcessAction::Resume),
            ProtectionLevel::Allowed
        );
    }

    #[test]
    fn own_webview_helpers_are_blocked_but_other_children_are_not() {
        let mut helper = win("msedgewebview2.exe", None);
        helper.parent_pid = Some(helper.self_pid);
        for action in [
            ProcessAction::Suspend,
            ProcessAction::Terminate,
            ProcessAction::ForceKill,
        ] {
            assert_eq!(
                level(Platform::Windows, &helper, action),
                ProtectionLevel::Blocked
            );
        }
        let mut other = win("notepad.exe", None);
        other.parent_pid = Some(other.self_pid);
        assert_eq!(
            level(Platform::Windows, &other, ProcessAction::Terminate),
            ProtectionLevel::Allowed
        );
        // The same name elsewhere (another app's WebView2) is not ours.
        let foreign = win("msedgewebview2.exe", None);
        assert_eq!(
            level(Platform::Windows, &foreign, ProcessAction::Terminate),
            ProtectionLevel::Allowed
        );
    }

    #[test]
    fn ordinary_processes_are_allowed() {
        let node = win("node.exe", Some(r"C:\Program Files\nodejs\node.exe"));
        let a = assess(Platform::Windows, &node, ProcessAction::Terminate);
        assert_eq!(a.level, ProtectionLevel::Allowed);
        assert!(a.reasons.is_empty());
    }

    #[test]
    fn linux_rules() {
        let base = ProcessFacts {
            pid: 1,
            parent_pid: Some(0),
            name: "systemd",
            user: Some("root"),
            self_pid: 4242,
            ..Default::default()
        };
        assert_eq!(
            level(Platform::Linux, &base, ProcessAction::ForceKill),
            ProtectionLevel::Blocked
        );

        let kthread = ProcessFacts {
            pid: 77,
            parent_pid: Some(2),
            name: "kworker/0:1",
            ..base.clone()
        };
        assert_eq!(
            level(Platform::Linux, &kthread, ProcessAction::Terminate),
            ProtectionLevel::Blocked
        );

        let logind = ProcessFacts {
            pid: 600,
            parent_pid: Some(1),
            name: "systemd-logind",
            ..base.clone()
        };
        assert_eq!(
            level(Platform::Linux, &logind, ProcessAction::Terminate),
            ProtectionLevel::Warning
        );

        let user_app = ProcessFacts {
            pid: 3000,
            parent_pid: Some(2500),
            name: "python3",
            user: Some("alice"),
            ..base
        };
        assert_eq!(
            level(Platform::Linux, &user_app, ProcessAction::ForceKill),
            ProtectionLevel::Allowed
        );

        let zombie = ProcessFacts {
            zombie: true,
            ..user_app.clone()
        };
        for action in [
            ProcessAction::Terminate,
            ProcessAction::ForceKill,
            ProcessAction::Suspend,
        ] {
            assert_eq!(
                level(Platform::Linux, &zombie, action),
                ProtectionLevel::Blocked
            );
        }

        // Kernel threads are identified by flag, not only by kthreadd parentage
        // (containers may not show PID 2).
        let flagged = ProcessFacts {
            kernel_thread: true,
            parent_pid: None,
            ..user_app
        };
        assert_eq!(
            level(Platform::Linux, &flagged, ProcessAction::Terminate),
            ProtectionLevel::Blocked
        );
    }

    #[test]
    fn high_priority_warns_everywhere() {
        let f = win("game.exe", None);
        let high = ProcessAction::SetPriority {
            priority: PriorityLevel::High,
        };
        assert_eq!(level(Platform::Windows, &f, high), ProtectionLevel::Warning);
    }
}
