//! Which network interface carries internet traffic, and whether a VPN or
//! tunnel is probably in the way. Pure parsing, so it is tested with fixtures.

use super::super::model::NetworkPath;
use crate::models::InterfaceKind;

const RTF_UP: u32 = 0x1;
const RTF_GATEWAY: u32 = 0x2;

/// The interface of the default route from `/proc/net/route`: the lowest
/// metric among routes to 0.0.0.0/0 that are up and use a gateway. Interface
/// names are never used to guess what an interface is.
pub fn default_route_interface(text: &str) -> Option<String> {
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let f: Vec<&str> = line.split_whitespace().collect();
            let (iface, dest, flags, metric) = (*f.first()?, *f.get(1)?, *f.get(3)?, *f.get(6)?);
            let flags = u32::from_str_radix(flags, 16).ok()?;
            let default = dest == "00000000" && flags & RTF_UP != 0 && flags & RTF_GATEWAY != 0;
            default.then(|| (metric.parse::<u32>().unwrap_or(u32::MAX), iface.to_owned()))
        })
        .min_by_key(|(metric, _)| *metric)
        .map(|(_, iface)| iface)
}

/// Builds the path description. `kind_of` classifies an interface from kernel
/// facts; `others` are all interfaces currently up with an address, so a
/// tunnel that is not the default route is still mentioned.
pub fn describe_path(
    default_interface: Option<String>,
    kind_of: &dyn Fn(&str) -> InterfaceKind,
    others: &[(String, InterfaceKind)],
) -> NetworkPath {
    let kind = default_interface.as_deref().map(kind_of);
    let vpn_suspected = kind == Some(InterfaceKind::Vpn)
        || others.iter().any(|(_, k)| *k == InterfaceKind::Vpn);
    NetworkPath {
        interface: default_interface,
        kind,
        vpn_suspected,
    }
}

#[cfg(target_os = "linux")]
pub fn read_default_route() -> Option<String> {
    default_route_interface(&std::fs::read_to_string("/proc/net/route").ok()?)
}

#[cfg(not(target_os = "linux"))]
pub fn read_default_route() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROUTES: &str = "Iface\tDestination\tGateway \tFlags\tRefCnt\tUse\tMetric\tMask\tMTU\tWindow\tIRTT\n\
        wlo1\t00000000\t0100A8C0\t0003\t0\t0\t600\t00000000\t0\t0\t0\n\
        wlo1\t0000A8C0\t00000000\t0001\t0\t0\t600\t00FFFFFF\t0\t0\t0\n\
        tailscale0\t00000000\t00000000\t0001\t0\t0\t0\t00000000\t0\t0\t0\n\
        docker0\t000011AC\t00000000\t0001\t0\t0\t0\t0000FFFF\t0\t0\t0\n";

    #[test]
    fn the_default_route_is_the_up_gateway_route_to_everywhere() {
        // tailscale0 has a 0.0.0.0 route but no gateway flag, so it is not it.
        assert_eq!(default_route_interface(ROUTES).as_deref(), Some("wlo1"));
    }

    #[test]
    fn the_lowest_metric_wins_among_several_defaults() {
        let t = "Iface Destination Gateway Flags RefCnt Use Metric Mask\n\
                 eth0 00000000 0100A8C0 0003 0 0 100 00000000\n\
                 wlan0 00000000 0100A8C0 0003 0 0 600 00000000\n";
        assert_eq!(default_route_interface(t).as_deref(), Some("eth0"));
    }

    #[test]
    fn a_down_route_and_garbage_are_ignored() {
        let down = "Iface Destination Gateway Flags RefCnt Use Metric Mask\n\
                    eth0 00000000 0100A8C0 0002 0 0 100 00000000\n";
        assert_eq!(default_route_interface(down), None, "not up");
        assert_eq!(default_route_interface(""), None);
        assert_eq!(default_route_interface("Iface\nbroken line\nx y z"), None);
        assert_eq!(default_route_interface("Iface Destination Gateway Flags\neth0 00000000 1 zz"), None);
    }

    #[test]
    fn a_vpn_default_route_or_a_tunnel_elsewhere_raises_the_warning() {
        let kind = |n: &str| match n {
            "tun0" | "wg0" => InterfaceKind::Vpn,
            "wlo1" => InterfaceKind::Wifi,
            _ => InterfaceKind::Unknown,
        };
        // Default route through the tunnel.
        let p = describe_path(Some("tun0".into()), &kind, &[]);
        assert_eq!((p.kind, p.vpn_suspected), (Some(InterfaceKind::Vpn), true));
        // Wi-Fi is the default, but a tunnel is up too.
        let p = describe_path(
            Some("wlo1".into()),
            &kind,
            &[("wlo1".into(), InterfaceKind::Wifi), ("wg0".into(), InterfaceKind::Vpn)],
        );
        assert_eq!(p.kind, Some(InterfaceKind::Wifi));
        assert!(p.vpn_suspected);
        // A plain connection.
        let p = describe_path(Some("wlo1".into()), &kind, &[("wlo1".into(), InterfaceKind::Wifi)]);
        assert!(!p.vpn_suspected);
        // No route at all is reported as unknown, not guessed.
        let p = describe_path(None, &kind, &[]);
        assert_eq!((p.interface, p.kind, p.vpn_suspected), (None, None, false));
    }
}
