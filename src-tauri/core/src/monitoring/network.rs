use std::collections::HashMap;
use std::net::IpAddr;
use std::time::Duration;

use sysinfo::Networks;

use crate::models::{InterfaceKind, NetworkInterfaceSnapshot};
use crate::monitoring::rates::counter_rate;
use crate::platform;

pub struct NetworkTracker {
    /// `None` when the platform cannot enumerate interfaces at all.
    networks: Option<Networks>,
    /// Previous (rx, tx) totals per interface name.
    previous: HashMap<String, (u64, u64)>,
    /// Classified once per name; an interface's kind does not change.
    kinds: HashMap<String, InterfaceKind>,
}

impl NetworkTracker {
    pub fn new() -> Self {
        let networks = Networks::new_with_refreshed_list()
            .inspect_err(|e| tracing::warn!("network enumeration unavailable: {e}"))
            .ok();
        let previous = networks
            .iter()
            .flat_map(|n| n.iter())
            .map(|(name, d)| {
                let name = name.to_string_lossy().into_owned();
                (name, (d.total_received(), d.total_transmitted()))
            })
            .collect();
        Self {
            networks,
            previous,
            kinds: HashMap::new(),
        }
    }

    pub fn sample(&mut self, elapsed: Duration) -> Vec<NetworkInterfaceSnapshot> {
        let Some(networks) = self.networks.as_mut() else {
            return Vec::new();
        };
        networks.refresh(true);

        let mut out: Vec<NetworkInterfaceSnapshot> = Vec::with_capacity(networks.len());
        let mut seen = HashMap::with_capacity(networks.len());
        for (name, data) in networks.iter() {
            let name = name.to_string_lossy().into_owned();
            let (rx_total, tx_total) = (data.total_received(), data.total_transmitted());
            seen.insert(name.clone(), (rx_total, tx_total));

            let ips: Vec<IpAddr> = data.ip_networks().iter().map(|n| n.addr).collect();
            if !is_relevant(&name, &ips, rx_total.saturating_add(tx_total)) {
                continue;
            }
            // Interfaces that appear mid-session start at a zero rate.
            let (rx_rate, tx_rate) = match self.previous.get(&name) {
                Some(&(prx, ptx)) => (
                    counter_rate(prx, rx_total, elapsed),
                    counter_rate(ptx, tx_total, elapsed),
                ),
                None => (0.0, 0.0),
            };
            let mac = data.mac_address();
            let kind = *self
                .kinds
                .entry(name.clone())
                .or_insert_with(|| platform::interface_kind(&name));
            if kind == InterfaceKind::Loopback {
                continue;
            }
            out.push(NetworkInterfaceSnapshot {
                name,
                rx_bytes_per_sec: rx_rate,
                tx_bytes_per_sec: tx_rate,
                total_rx_bytes: rx_total,
                total_tx_bytes: tx_total,
                ipv4: ips
                    .iter()
                    .filter(|ip| ip.is_ipv4())
                    .map(ToString::to_string)
                    .collect(),
                ipv6: ips
                    .iter()
                    .filter(|ip| ip.is_ipv6())
                    .map(ToString::to_string)
                    .collect(),
                mac_address: (!mac.is_unspecified()).then(|| mac.to_string()),
                is_primary: false,
                kind,
            });
        }
        self.previous = seen;
        // Forget interfaces that went away (names can be reused by a
        // different kind of device, e.g. a new VPN).
        self.kinds
            .retain(|name, _| self.previous.contains_key(name));

        mark_primary(&mut out);
        out.sort_by(|a, b| b.is_primary.cmp(&a.is_primary).then(a.name.cmp(&b.name)));
        out
    }
}

/// Hides loopback devices and Windows NDIS filter/miniport pseudo-adapters,
/// which duplicate the counters of the real adapter they are bound to.
pub fn is_relevant(name: &str, ips: &[IpAddr], total_bytes: u64) -> bool {
    let lower = name.to_ascii_lowercase();
    if lower == "lo" || lower.starts_with("loopback") {
        return false;
    }
    const FILTER_MARKERS: &[&str] = &[
        "-wfp ",
        "lightweight filter",
        "qos packet scheduler",
        "-native wifi filter",
        "-virtual wifi filter",
        "npcap packet driver",
        "kernel debug",
    ];
    if FILTER_MARKERS.iter().any(|m| lower.contains(m)) {
        return false;
    }
    if !ips.is_empty() && ips.iter().all(IpAddr::is_loopback) {
        return false;
    }
    !ips.is_empty() || total_bytes > 0
}

/// The primary interface is the one with a routable IPv4 address and the
/// most lifetime traffic.
fn mark_primary(interfaces: &mut [NetworkInterfaceSnapshot]) {
    let best = interfaces
        .iter()
        .enumerate()
        .filter(|(_, i)| {
            i.kind.counts_in_totals()
                && i.ipv4
                    .iter()
                    .any(|ip| !ip.starts_with("169.254.") && !ip.starts_with("127."))
        })
        .max_by_key(|(_, i)| i.total_rx_bytes.saturating_add(i.total_tx_bytes))
        .map(|(idx, _)| idx);
    if let Some(idx) = best {
        interfaces[idx].is_primary = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn iface(name: &str, ipv4: &[&str], total: u64) -> NetworkInterfaceSnapshot {
        NetworkInterfaceSnapshot {
            name: name.into(),
            rx_bytes_per_sec: 0.0,
            tx_bytes_per_sec: 0.0,
            total_rx_bytes: total,
            total_tx_bytes: 0,
            ipv4: ipv4.iter().map(|s| s.to_string()).collect(),
            ipv6: vec![],
            mac_address: None,
            is_primary: false,
            kind: InterfaceKind::Unknown,
        }
    }

    #[test]
    fn hides_loopback_and_filter_adapters() {
        let lo: IpAddr = "127.0.0.1".parse().unwrap();
        let lan: IpAddr = "192.168.1.5".parse().unwrap();
        assert!(!is_relevant("lo", &[lo], 10));
        assert!(!is_relevant(
            "Ethernet-WFP Native MAC Layer LightWeight Filter-0000",
            &[],
            10
        ));
        assert!(!is_relevant("Wi-Fi-QoS Packet Scheduler-0000", &[], 10));
        assert!(is_relevant("Ethernet", &[lan], 0));
        assert!(is_relevant("eth0", &[], 5));
        assert!(!is_relevant("unused0", &[], 0));
    }

    #[test]
    fn primary_prefers_routable_ipv4_with_most_traffic() {
        let mut list = vec![
            iface("docker0", &["172.17.0.1"], 10),
            iface("eth0", &["192.168.1.2"], 5_000),
            iface("wlan0", &["169.254.3.4"], 99_999),
        ];
        mark_primary(&mut list);
        assert!(list[1].is_primary);
        assert!(!list[0].is_primary && !list[2].is_primary);
    }
}
