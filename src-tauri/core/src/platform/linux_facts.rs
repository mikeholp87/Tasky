//! Pure interpretation of Linux system facts: os-release, session
//! environment, container markers and `/sys/class/net` attributes. The
//! readers live in `platform/linux`; these functions take plain values so
//! they are unit-tested on every OS.

use crate::models::InterfaceKind;

/// `PRETTY_NAME` from an os-release(5) file, falling back to `NAME VERSION`.
pub fn os_release_pretty_name(text: &str) -> Option<String> {
    let mut pretty = None;
    let (mut name, mut version) = (None, None);
    for line in text.lines() {
        let Some((key, value)) = line.trim().split_once('=') else {
            continue;
        };
        let value = unquote(value.trim());
        if value.is_empty() {
            continue;
        }
        match key {
            "PRETTY_NAME" => pretty = Some(value),
            "NAME" => name = Some(value),
            "VERSION" => version = Some(value),
            _ => {}
        }
    }
    pretty.or(match (name, version) {
        (Some(n), Some(v)) => Some(format!("{n} {v}")),
        (n, _) => n,
    })
}

/// Shell-style quoting as allowed by os-release(5): one level of single or
/// double quotes, with backslash escapes inside double quotes.
fn unquote(value: &str) -> String {
    let bytes = value.as_bytes();
    if bytes.len() >= 2 && bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\'' {
        return value[1..value.len() - 1].to_owned();
    }
    if !(bytes.len() >= 2 && bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"') {
        return value.to_owned();
    }
    let mut out = String::with_capacity(value.len());
    let mut chars = value[1..value.len() - 1].chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Human name for `XDG_CURRENT_DESKTOP` (a colon-separated list such as
/// `ubuntu:GNOME`), falling back to `XDG_SESSION_DESKTOP`/`DESKTOP_SESSION`.
pub fn desktop_environment(current: Option<&str>, session: Option<&str>) -> Option<String> {
    const KNOWN: &[(&str, &str)] = &[
        ("kde", "KDE Plasma"),
        ("plasma", "KDE Plasma"),
        ("gnome", "GNOME"),
        ("gnome-classic", "GNOME Classic"),
        ("xfce", "Xfce"),
        ("x-cinnamon", "Cinnamon"),
        ("cinnamon", "Cinnamon"),
        ("mate", "MATE"),
        ("lxqt", "LXQt"),
        ("lxde", "LXDE"),
        ("budgie", "Budgie"),
        ("budgie-desktop", "Budgie"),
        ("cosmic", "COSMIC"),
        ("pantheon", "Pantheon"),
        ("unity", "Unity"),
        ("deepin", "Deepin"),
        ("sway", "Sway"),
        ("hyprland", "Hyprland"),
    ];
    let tokens = || {
        current
            .into_iter()
            .flat_map(|c| c.split(':'))
            .chain(session)
            .map(str::trim)
            .filter(|t| !t.is_empty())
    };
    tokens()
        .find_map(|t| {
            let lower = t.to_ascii_lowercase();
            KNOWN
                .iter()
                .find(|(k, _)| *k == lower)
                .map(|(_, v)| (*v).to_owned())
        })
        .or_else(|| tokens().next().map(str::to_owned))
}

/// "Wayland" or "X11" for the session. `XDG_SESSION_TYPE` wins; the display
/// variables are a fallback for sessions started without logind.
pub fn display_server(
    session_type: Option<&str>,
    wayland_display: Option<&str>,
    display: Option<&str>,
) -> Option<String> {
    let set = |v: Option<&str>| v.is_some_and(|s| !s.is_empty());
    match session_type.map(str::to_ascii_lowercase).as_deref() {
        Some("wayland") => Some("Wayland".into()),
        Some("x11") => Some("X11".into()),
        _ if set(wayland_display) => Some("Wayland".into()),
        _ if set(display) => Some("X11".into()),
        _ => None,
    }
}

/// Container detection from the markers container runtimes leave behind.
/// `systemd_container` is `/run/systemd/container`, written by systemd for
/// docker, podman, lxc, systemd-nspawn, wsl and others.
pub fn container(
    systemd_container: Option<&str>,
    has_dockerenv: bool,
    has_containerenv: bool,
    kernel_osrelease: Option<&str>,
) -> Option<String> {
    let label = |id: &str| match id {
        "docker" => "Docker".to_owned(),
        "podman" => "Podman".to_owned(),
        "lxc" | "lxc-libvirt" => "LXC".to_owned(),
        "systemd-nspawn" => "systemd-nspawn".to_owned(),
        "wsl" => "WSL".to_owned(),
        other => other.to_owned(),
    };
    if let Some(id) = systemd_container.map(str::trim).filter(|s| !s.is_empty()) {
        return Some(label(id));
    }
    if has_dockerenv {
        return Some("Docker".into());
    }
    if has_containerenv {
        return Some("Podman".into());
    }
    kernel_osrelease
        .is_some_and(|r| r.to_ascii_lowercase().contains("microsoft"))
        .then(|| "WSL".into())
}

/// One entry of `/sys/class/power_supply`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PowerSupply {
    /// `type`: "Mains", "Battery", "USB", ...
    pub kind: String,
    /// `online` (mains-type supplies).
    pub online: Option<bool>,
    /// `status` (batteries): "Charging", "Discharging", "Full", ...
    pub status: Option<String>,
}

/// AC or battery from the power supplies. No entries means a desktop or an
/// unreadable sysfs, which is "unknown", not "AC".
pub fn power_source(supplies: &[PowerSupply]) -> crate::benchmarks::models::PowerSource {
    use crate::benchmarks::models::PowerSource;
    if supplies
        .iter()
        .any(|s| s.kind.eq_ignore_ascii_case("mains") && s.online == Some(true))
    {
        return PowerSource::Ac;
    }
    let battery = supplies
        .iter()
        .find(|s| s.kind.eq_ignore_ascii_case("battery"));
    match battery.and_then(|b| b.status.as_deref()) {
        Some(st) if st.eq_ignore_ascii_case("discharging") => PowerSource::Battery,
        // A battery that is charging or full is on external power.
        Some(st) if st.eq_ignore_ascii_case("charging") || st.eq_ignore_ascii_case("full") => {
            PowerSource::Ac
        }
        _ if battery.is_some() && supplies.iter().any(|s| s.online == Some(false)) => {
            PowerSource::Battery
        }
        _ => PowerSource::Unknown,
    }
}

/// Whether `/proc/cpuinfo` lists the `hypervisor` CPU flag, which the kernel
/// sets when it runs under a hypervisor.
pub fn cpuinfo_has_hypervisor(text: &str) -> bool {
    text.lines()
        .filter(|l| l.starts_with("flags"))
        .any(|l| l.split_whitespace().any(|f| f == "hypervisor"))
}

/// Attributes of `/sys/class/net/<iface>` that identify its kind.
#[derive(Debug, Clone, Default)]
pub struct NetFacts {
    /// `type`: ARPHRD_* hardware type.
    pub arp_type: Option<u32>,
    /// `wireless/` or `phy80211` exists.
    pub wireless: bool,
    /// `tun_flags`, present only on TUN/TAP devices.
    pub tun_flags: Option<u32>,
    /// `bridge/` exists.
    pub bridge: bool,
    /// `device` link exists: backed by a bus device (PCI, USB, SDIO, …).
    pub has_device: bool,
    /// `DEVTYPE` from `uevent`, e.g. "wlan", "wireguard", "vlan", "bond".
    pub devtype: Option<String>,
}

const ARPHRD_ETHER: u32 = 1;
const ARPHRD_LOOPBACK: u32 = 772;
const IFF_TUN: u32 = 0x0001;
const IFF_TAP: u32 = 0x0002;
/// ARPHRD_TUNNEL, TUNNEL6, SIT, IPGRE, IP6GRE, NONE (WireGuard, TUN).
const TUNNEL_TYPES: &[u32] = &[768, 769, 776, 778, 823, 65534];

pub fn classify_interface(f: &NetFacts) -> InterfaceKind {
    let devtype = f.devtype.as_deref().unwrap_or("");
    if f.arp_type == Some(ARPHRD_LOOPBACK) {
        return InterfaceKind::Loopback;
    }
    if f.wireless || devtype == "wlan" {
        return InterfaceKind::Wifi;
    }
    if devtype == "wireguard" {
        return InterfaceKind::Vpn;
    }
    if let Some(flags) = f.tun_flags {
        if flags & IFF_TUN != 0 {
            return InterfaceKind::Vpn;
        }
        if flags & IFF_TAP != 0 {
            return InterfaceKind::Virtual;
        }
    }
    if f.arp_type.is_some_and(|t| TUNNEL_TYPES.contains(&t)) {
        return InterfaceKind::Vpn;
    }
    // Masters and stacked devices re-count their members' traffic.
    if f.bridge || matches!(devtype, "bridge" | "vlan" | "bond" | "macvlan" | "ipvlan") {
        return InterfaceKind::Virtual;
    }
    match f.arp_type {
        Some(ARPHRD_ETHER) if f.has_device => InterfaceKind::Ethernet,
        // veth, dummy, docker0-style software Ethernet.
        Some(ARPHRD_ETHER) => InterfaceKind::Virtual,
        _ => InterfaceKind::Unknown,
    }
}

/// Parses a sysfs integer, decimal or `0x` hex (as in `tun_flags`).
pub fn parse_sysfs_u32(text: &str) -> Option<u32> {
    let t = text.trim();
    match t.strip_prefix("0x") {
        Some(hex) => u32::from_str_radix(hex, 16).ok(),
        None => t.parse().ok(),
    }
}

/// `DEVTYPE=` from a sysfs `uevent` file.
pub fn uevent_devtype(text: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.strip_prefix("DEVTYPE="))
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}

/// Firmware placeholder values that carry no information.
fn is_placeholder(value: &str) -> bool {
    let v = value.trim().to_ascii_lowercase();
    v.is_empty()
        || matches!(
            v.as_str(),
            "to be filled by o.e.m."
                | "to be filled by oem"
                | "default string"
                | "system product name"
                | "system manufacturer"
                | "system version"
                | "not specified"
                | "not applicable"
                | "none"
                | "unknown"
                | "o.e.m."
                | "oem"
        )
}

/// A DMI string, or `None` when it is empty or a vendor placeholder.
pub fn clean_dmi(text: &str) -> Option<String> {
    let t = text.trim();
    (!is_placeholder(t)).then(|| t.to_owned())
}

/// SMBIOS chassis type code (`/sys/class/dmi/id/chassis_type`) as a name.
pub fn chassis_name(code: &str) -> Option<&'static str> {
    Some(match code.trim().parse::<u32>().ok()? {
        3 | 4 | 5 | 6 | 7 | 15 | 16 => "Desktop",
        8 | 9 | 10 | 14 => "Laptop",
        11 => "Handheld",
        13 => "All-in-one",
        17 | 23 | 25 | 28 => "Server",
        30 => "Tablet",
        31 | 32 => "Convertible",
        35 | 36 => "Mini PC",
        _ => return None,
    })
}

/// A sysfs cache size such as `32K`, `1024K` or `8M`, in bytes.
pub fn parse_cache_size(text: &str) -> Option<u64> {
    let t = text.trim();
    let split = t.find(|c: char| !c.is_ascii_digit()).unwrap_or(t.len());
    let (digits, unit) = t.split_at(split);
    let n: u64 = digits.parse().ok()?;
    match unit.trim().to_ascii_uppercase().as_str() {
        "K" => n.checked_mul(1024),
        "M" => n.checked_mul(1024 * 1024),
        "" => Some(n),
        _ => None,
    }
}

/// Hardware virtualization the CPU offers, from the `flags` line of `/proc/cpuinfo`.
pub fn cpu_virtualization(cpuinfo: &str) -> Option<&'static str> {
    let flags = cpuinfo.lines().find(|l| l.starts_with("flags"))?;
    let has = |f: &str| flags.split_whitespace().any(|x| x == f);
    if has("vmx") {
        Some("Intel VT-x")
    } else if has("svm") {
        Some("AMD-V")
    } else {
        None
    }
}

/// Full-charge capacity relative to design capacity, as a percentage.
pub fn battery_health(full: u64, design: u64) -> Option<u8> {
    (design > 0 && full > 0).then(|| ((full * 100 + design / 2) / design).min(100) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_source_from_supplies() {
        use crate::benchmarks::models::PowerSource as P;
        let mains = |on| PowerSupply {
            kind: "Mains".into(),
            online: Some(on),
            status: None,
        };
        let battery = |st: &str| PowerSupply {
            kind: "Battery".into(),
            online: None,
            status: Some(st.into()),
        };
        assert_eq!(power_source(&[mains(true), battery("Charging")]), P::Ac);
        assert_eq!(
            power_source(&[mains(false), battery("Discharging")]),
            P::Battery
        );
        assert_eq!(power_source(&[battery("Discharging")]), P::Battery);
        assert_eq!(power_source(&[battery("Full")]), P::Ac);
        // Battery with no usable status but mains offline.
        assert_eq!(
            power_source(&[mains(false), battery("Unknown")]),
            P::Battery
        );
        // Desktop: nothing to go on.
        assert_eq!(power_source(&[]), P::Unknown);
        assert_eq!(power_source(&[battery("Unknown")]), P::Unknown);
    }

    #[test]
    fn hypervisor_flag() {
        assert!(cpuinfo_has_hypervisor(
            "processor : 0\nflags : fpu vme hypervisor lahf_lm\n"
        ));
        assert!(!cpuinfo_has_hypervisor(
            "flags : fpu vme sse\nmodel name : hypervisor-like\n"
        ));
        assert!(!cpuinfo_has_hypervisor(""));
    }

    #[test]
    fn os_release_variants() {
        let debian = "PRETTY_NAME=\"Debian GNU/Linux 13 (trixie)\"\nNAME=\"Debian GNU/Linux\"\nVERSION_ID=\"13\"\n";
        assert_eq!(
            os_release_pretty_name(debian).as_deref(),
            Some("Debian GNU/Linux 13 (trixie)")
        );
        let sid = "PRETTY_NAME=\"Debian GNU/Linux forky/sid\"\nNAME=\"Debian GNU/Linux\"\n";
        assert_eq!(
            os_release_pretty_name(sid).as_deref(),
            Some("Debian GNU/Linux forky/sid")
        );
        // No PRETTY_NAME: NAME + VERSION; single quotes; escapes; comments.
        let partial = "# comment\nNAME='Foo OS'\nVERSION=\"2 \\\"LTS\\\"\"\n";
        assert_eq!(
            os_release_pretty_name(partial).as_deref(),
            Some("Foo OS 2 \"LTS\"")
        );
        assert_eq!(os_release_pretty_name("garbage\n=\n"), None);
        assert_eq!(
            os_release_pretty_name("PRETTY_NAME=\"\"\nNAME=X").as_deref(),
            Some("X")
        );
    }

    #[test]
    fn desktop_names() {
        assert_eq!(
            desktop_environment(Some("KDE"), None).as_deref(),
            Some("KDE Plasma")
        );
        assert_eq!(
            desktop_environment(Some("ubuntu:GNOME"), None).as_deref(),
            Some("GNOME")
        );
        assert_eq!(
            desktop_environment(Some("X-Cinnamon"), None).as_deref(),
            Some("Cinnamon")
        );
        assert_eq!(
            desktop_environment(None, Some("xfce")).as_deref(),
            Some("Xfce")
        );
        assert_eq!(
            desktop_environment(Some("Niri"), None).as_deref(),
            Some("Niri")
        );
        assert_eq!(desktop_environment(Some(""), None), None);
        assert_eq!(desktop_environment(None, None), None);
    }

    #[test]
    fn display_server_detection() {
        assert_eq!(
            display_server(Some("wayland"), None, Some(":1")).as_deref(),
            Some("Wayland")
        );
        assert_eq!(
            display_server(Some("x11"), None, Some(":0")).as_deref(),
            Some("X11")
        );
        // tty session type but a display is set (e.g. startx).
        assert_eq!(
            display_server(Some("tty"), None, Some(":0")).as_deref(),
            Some("X11")
        );
        assert_eq!(
            display_server(None, Some("wayland-0"), None).as_deref(),
            Some("Wayland")
        );
        assert_eq!(display_server(None, Some(""), Some("")), None);
    }

    #[test]
    fn container_detection() {
        assert_eq!(
            container(Some("podman\n"), true, false, None).as_deref(),
            Some("Podman")
        );
        assert_eq!(
            container(None, true, false, None).as_deref(),
            Some("Docker")
        );
        assert_eq!(
            container(None, false, true, None).as_deref(),
            Some("Podman")
        );
        assert_eq!(
            container(
                None,
                false,
                false,
                Some("5.15.153.1-microsoft-standard-WSL2")
            )
            .as_deref(),
            Some("WSL")
        );
        assert_eq!(container(None, false, false, Some("6.12.0-amd64")), None);
        assert_eq!(container(Some(""), false, false, None), None);
    }

    fn facts(arp: u32) -> NetFacts {
        NetFacts {
            arp_type: Some(arp),
            ..NetFacts::default()
        }
    }

    #[test]
    fn interface_classification() {
        use InterfaceKind as K;
        assert_eq!(classify_interface(&facts(772)), K::Loopback);
        let eth = NetFacts {
            has_device: true,
            ..facts(1)
        };
        assert_eq!(classify_interface(&eth), K::Ethernet);
        let wifi = NetFacts {
            has_device: true,
            wireless: true,
            ..facts(1)
        };
        assert_eq!(classify_interface(&wifi), K::Wifi);
        let wg = NetFacts {
            devtype: Some("wireguard".into()),
            ..facts(65534)
        };
        assert_eq!(classify_interface(&wg), K::Vpn);
        // tailscale0 / OpenVPN tun.
        let tun = NetFacts {
            tun_flags: Some(0x1001),
            ..facts(65534)
        };
        assert_eq!(classify_interface(&tun), K::Vpn);
        let tap = NetFacts {
            tun_flags: Some(0x1002),
            ..facts(1)
        };
        assert_eq!(classify_interface(&tap), K::Virtual);
        let docker0 = NetFacts {
            bridge: true,
            devtype: Some("bridge".into()),
            ..facts(1)
        };
        assert_eq!(classify_interface(&docker0), K::Virtual);
        // veth has no bus device.
        assert_eq!(classify_interface(&facts(1)), K::Virtual);
        let vlan = NetFacts {
            devtype: Some("vlan".into()),
            ..facts(1)
        };
        assert_eq!(classify_interface(&vlan), K::Virtual);
        assert_eq!(classify_interface(&facts(776)), K::Vpn);
        assert_eq!(classify_interface(&NetFacts::default()), K::Unknown);
    }

    #[test]
    fn sysfs_values() {
        assert_eq!(parse_sysfs_u32("0x1001\n"), Some(0x1001));
        assert_eq!(parse_sysfs_u32("772\n"), Some(772));
        assert_eq!(parse_sysfs_u32("x"), None);
        assert_eq!(
            uevent_devtype("DEVTYPE=wlan\nINTERFACE=wlo1\n").as_deref(),
            Some("wlan")
        );
        assert_eq!(uevent_devtype("INTERFACE=eth0\n"), None);
    }

    #[test]
    fn cleans_dmi_strings() {
        assert_eq!(clean_dmi(" LENOVO\n"), Some("LENOVO".into()));
        assert_eq!(clean_dmi("To Be Filled By O.E.M."), None);
        assert_eq!(clean_dmi("Default string"), None);
        assert_eq!(clean_dmi("  "), None);
    }

    #[test]
    fn names_chassis_types() {
        assert_eq!(chassis_name("10\n"), Some("Laptop"));
        assert_eq!(chassis_name("3"), Some("Desktop"));
        assert_eq!(chassis_name("1"), None);
        assert_eq!(chassis_name("x"), None);
    }

    #[test]
    fn parses_cache_sizes() {
        assert_eq!(parse_cache_size("32K\n"), Some(32 * 1024));
        assert_eq!(parse_cache_size("8M"), Some(8 * 1024 * 1024));
        assert_eq!(parse_cache_size("512"), Some(512));
        assert_eq!(parse_cache_size("lots"), None);
        assert_eq!(parse_cache_size(""), None);
    }

    #[test]
    fn detects_cpu_virtualization_flags() {
        let intel = "processor : 0\nflags\t\t: fpu vme vmx sse\n";
        let amd = "flags : fpu svm";
        assert_eq!(cpu_virtualization(intel), Some("Intel VT-x"));
        assert_eq!(cpu_virtualization(amd), Some("AMD-V"));
        assert_eq!(cpu_virtualization("flags : fpu sse"), None);
        assert_eq!(cpu_virtualization("model name : x"), None);
    }

    #[test]
    fn computes_battery_health() {
        assert_eq!(battery_health(42_000, 50_000), Some(84));
        assert_eq!(battery_health(51_000, 50_000), Some(100), "capped");
        assert_eq!(battery_health(0, 50_000), None);
        assert_eq!(battery_health(1, 0), None);
    }
}
