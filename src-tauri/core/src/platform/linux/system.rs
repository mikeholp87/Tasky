//! Static facts about the OS and session, read once at startup.

use std::path::Path;

use super::super::{OsDetails, linux_facts as facts};

pub fn os_details() -> OsDetails {
    let env = |k: &str| std::env::var(k).ok();
    let read = |p: &str| std::fs::read_to_string(p).ok();
    OsDetails {
        // os-release(5): /etc first, /usr/lib as the vendor fallback.
        distribution: read("/etc/os-release")
            .or_else(|| read("/usr/lib/os-release"))
            .and_then(|t| facts::os_release_pretty_name(&t)),
        desktop_environment: facts::desktop_environment(
            env("XDG_CURRENT_DESKTOP").as_deref(),
            env("XDG_SESSION_DESKTOP")
                .or_else(|| env("DESKTOP_SESSION"))
                .as_deref(),
        ),
        display_server: facts::display_server(
            env("XDG_SESSION_TYPE").as_deref(),
            env("WAYLAND_DISPLAY").as_deref(),
            env("DISPLAY").as_deref(),
        ),
        container: facts::container(
            read("/run/systemd/container").as_deref(),
            Path::new("/.dockerenv").exists(),
            Path::new("/run/.containerenv").exists(),
            read("/proc/sys/kernel/osrelease").as_deref(),
        ),
        // /proc/1/comm is world-readable; /run/systemd/system is sd_booted().
        init_system: if Path::new("/run/systemd/system").is_dir() {
            Some("systemd".into())
        } else {
            read("/proc/1/comm")
                .map(|c| c.trim().to_owned())
                .filter(|c| !c.is_empty())
        },
    }
}
