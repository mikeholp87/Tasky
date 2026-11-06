//! Internet quality benchmark.
//!
//! Measurements come from a [`NetworkProvider`]. The default provider is
//! the official `@cloudflare/speedtest` package, which must run in the
//! WebView (it relies on the browser's Resource Timing API): Rust sends it
//! a measurement plan sized to the selected data profile, the WebView
//! reports progress and finally submits the raw samples, and Rust validates
//! every value and computes all statistics itself.
//!
//! * Bandwidth (Mbps, 10^6 bit/s) = bytes × 8 / transfer seconds per
//!   request. The headline is the 90th percentile of transfers lasting at
//!   least [`MIN_BANDWIDTH_DURATION_MS`]: small requests are dominated by
//!   latency, and the upper percentile approximates available capacity
//!   (the same reduction Cloudflare uses).
//! * Latency = median RTT; jitter = mean absolute difference of consecutive
//!   RTTs. Loaded latency is measured while download / upload traffic runs.
//! * Packet loss is reported only when the provider really measured it.

pub mod inbox;

use std::sync::Arc;
use std::time::{Duration, Instant};

use inbox::{InboxMessage, NetworkInbox};

use super::models::{
    BenchmarkEvent, BenchmarkKind, BenchmarkPhase, BenchmarkWarning, CategoryResult,
    CategoryStatus, Detail, MetricUnit, NetworkDataProfile, NetworkMeasurementKind,
    NetworkMeasurementSpec, NetworkSubmission, TransferSample, WarningCode,
};
use super::statistics::{mean_consecutive_difference, percentile, summarize};
use super::suite::{CategoryBenchmark, RunContext, metric_from_samples, metric_value};
use crate::errors::{AppError, AppResult};
use crate::models::NetworkInterfaceSnapshot;

/// Transfers shorter than this are latency-limited and excluded from the
/// bandwidth headline (they remain in the raw samples).
pub const MIN_BANDWIDTH_DURATION_MS: f64 = 250.0;
/// Minimum idle-latency samples for a meaningful median and jitter.
pub const MIN_LATENCY_SAMPLES: usize = 20;
/// Most entries accepted per sample list from the WebView.
pub const MAX_SAMPLES: usize = 2_000;
/// Loaded latency this much above idle (and at least +30 ms) is called out.
const LOADED_LATENCY_FACTOR: f64 = 2.0;
const LOADED_LATENCY_MIN_INCREASE_MS: f64 = 30.0;

const KB: u64 = 1_000;
const MB: u64 = 1_000_000;

fn step(kind: NetworkMeasurementKind, bytes: u64, count: u32) -> NetworkMeasurementSpec {
    NetworkMeasurementSpec { kind, bytes, count }
}

/// The measurement sequence for a data profile. Payloads grow in size so
/// latency-limited and bandwidth-limited transfers can be told apart; the
/// provider stops escalating once a size takes over a second, so slow
/// connections use less than the maximum.
pub fn plan(profile: NetworkDataProfile) -> Vec<NetworkMeasurementSpec> {
    use NetworkMeasurementKind::{Download as D, Latency as L, Upload as U};
    let mut p = vec![
        step(L, 0, 20),
        step(D, 100 * KB, 5),
        step(U, 100 * KB, 4),
        step(D, MB, 4),
        step(U, MB, 3),
    ];
    match profile {
        NetworkDataProfile::DataSaver => p.extend([step(D, 10 * MB, 1), step(L, 0, 10)]),
        NetworkDataProfile::Standard => p.extend([
            step(D, 10 * MB, 6),
            step(U, 10 * MB, 3),
            step(D, 25 * MB, 2),
            step(L, 0, 10),
        ]),
        NetworkDataProfile::Maximum => p.extend([
            step(D, 10 * MB, 6),
            step(U, 10 * MB, 6),
            step(D, 25 * MB, 2),
            step(U, 25 * MB, 4),
            step(D, 100 * MB, 4),
            step(L, 0, 10),
        ]),
    }
    p
}

/// Upper bound of payload bytes in a plan.
pub fn plan_bytes(plan: &[NetworkMeasurementSpec]) -> u64 {
    plan.iter().map(|s| s.bytes * u64::from(s.count)).sum()
}

pub fn mbps(bytes: f64, duration_ms: f64) -> Option<f64> {
    (bytes > 0.0 && duration_ms > 0.0).then(|| bytes * 8.0 / (duration_ms / 1_000.0) / 1e6)
}

/// Rejects malformed or implausible submissions before any statistic is
/// computed (the WebView is treated as untrusted input).
pub fn validate(s: &NetworkSubmission, max_bytes: u64) -> AppResult<()> {
    let invalid = |m: &str| {
        Err(AppError::InvalidArgument(format!(
            "Invalid network results: {m}."
        )))
    };
    let lists = [
        s.idle_latency_ms.len(),
        s.download_loaded_latency_ms.len(),
        s.upload_loaded_latency_ms.len(),
        s.download.len(),
        s.upload.len(),
    ];
    if lists.iter().any(|n| *n > MAX_SAMPLES) {
        return invalid("too many samples");
    }
    let latency_ok = |v: &f64| v.is_finite() && *v > 0.0 && *v < 60_000.0;
    if !s
        .idle_latency_ms
        .iter()
        .chain(&s.download_loaded_latency_ms)
        .chain(&s.upload_loaded_latency_ms)
        .all(latency_ok)
    {
        return invalid("latency out of range");
    }
    let transfer_ok = |t: &TransferSample| {
        t.bytes.is_finite()
            && t.duration_ms.is_finite()
            && t.bytes > 0.0
            && t.bytes <= 1e9
            && t.duration_ms > 0.0
            && t.duration_ms < 600_000.0
    };
    if !s.download.iter().chain(&s.upload).all(transfer_ok) {
        return invalid("transfer out of range");
    }
    let total: f64 = s.download.iter().chain(&s.upload).map(|t| t.bytes).sum();
    // Allow headers/rounding slack, but never more than the plan permits.
    if total > max_bytes as f64 * 1.1 + 5e6 {
        return invalid("more data than the selected profile allows");
    }
    if let Some(p) = s.packet_loss
        && (p.sent == 0 || p.lost > p.sent)
    {
        return invalid("packet loss counts");
    }
    if s.provider_version.len() > 64 || s.error.as_ref().is_some_and(|e| e.len() > 500) {
        return invalid("metadata too long");
    }
    Ok(())
}

/// Bandwidth metric: per-request Mbps; headline = p90 of long transfers.
fn bandwidth_metric(
    id: &str,
    label: &str,
    samples: &[TransferSample],
) -> Option<super::models::Metric> {
    let all: Vec<f64> = samples
        .iter()
        .filter_map(|t| mbps(t.bytes, t.duration_ms))
        .collect();
    if all.is_empty() {
        return None;
    }
    let long: Vec<f64> = samples
        .iter()
        .filter(|t| t.duration_ms >= MIN_BANDWIDTH_DURATION_MS)
        .filter_map(|t| mbps(t.bytes, t.duration_ms))
        .collect();
    let basis = if long.is_empty() { &all } else { &long };
    let mut m = metric_from_samples(id, label, MetricUnit::Mbps, true, basis.clone());
    m.value = percentile(basis, 0.9)?;
    m.note = Some(if long.is_empty() {
        "90th percentile of all transfers (none lasted 250 ms; the connection may be faster \
         than the largest payload can show)."
            .into()
    } else {
        format!(
            "90th percentile of {} transfers lasting ≥ 250 ms ({} transfers in total).",
            long.len(),
            all.len()
        )
    });
    Some(m)
}

fn latency_metrics(
    prefix: &str,
    label: &str,
    samples: &[f64],
    out: &mut Vec<super::models::Metric>,
) -> Option<f64> {
    let s = summarize(samples)?;
    let mut m = metric_from_samples(
        &format!("net.latency.{prefix}"),
        &format!("{label} latency"),
        MetricUnit::Milliseconds,
        false,
        samples.to_vec(),
    );
    m.note = Some(format!(
        "Median of {} round trips; minimum {:.1} ms, p95 {:.1} ms.",
        s.n, s.min, s.p95
    ));
    out.push(m);
    if let Some(j) = mean_consecutive_difference(samples) {
        out.push(metric_value(
            &format!("net.jitter.{prefix}"),
            &format!("{label} jitter"),
            MetricUnit::Milliseconds,
            false,
            j,
        ));
    }
    Some(s.median)
}

/// Turns validated samples into the category result.
pub fn analyze(s: &NetworkSubmission) -> CategoryResult {
    let mut metrics = Vec::new();
    let mut warnings = Vec::new();
    metrics.extend(bandwidth_metric("net.download", "Download", &s.download));
    metrics.extend(bandwidth_metric("net.upload", "Upload", &s.upload));
    let idle = latency_metrics("idle", "Idle", &s.idle_latency_ms, &mut metrics);
    let down = latency_metrics(
        "download",
        "Download-loaded",
        &s.download_loaded_latency_ms,
        &mut metrics,
    );
    let up = latency_metrics(
        "upload",
        "Upload-loaded",
        &s.upload_loaded_latency_ms,
        &mut metrics,
    );
    if s.idle_latency_ms.len() < MIN_LATENCY_SAMPLES {
        warnings.push(BenchmarkWarning::new(
            WarningCode::InsufficientSamples,
            format!(
                "Only {} idle latency samples were collected; latency and jitter are less \
                 reliable.",
                s.idle_latency_ms.len()
            ),
        ));
    }
    if let Some(idle) = idle {
        let increased = [down, up].into_iter().flatten().any(|loaded| {
            loaded > idle * LOADED_LATENCY_FACTOR && loaded - idle > LOADED_LATENCY_MIN_INCREASE_MS
        });
        if increased {
            warnings.push(BenchmarkWarning::new(
                WarningCode::LatencyIncreasedUnderLoad,
                "Latency increased substantially while the connection was busy.",
            ));
        }
    }
    let mut details = Vec::new();
    match s.packet_loss {
        Some(p) => metrics.push(metric_value(
            "net.packet_loss",
            "Packet loss",
            MetricUnit::Percent,
            false,
            f64::from(p.lost) / f64::from(p.sent) * 100.0,
        )),
        None => details.push(Detail::new(
            "packetLoss",
            "Packet loss",
            "Packet loss measurement unavailable with this provider.",
        )),
    }
    let transferred: f64 = s.download.iter().chain(&s.upload).map(|t| t.bytes).sum();
    metrics.push(metric_value(
        "net.bytes_transferred",
        "Data transferred",
        MetricUnit::Bytes,
        false,
        transferred,
    ));
    let mut r = CategoryResult::empty(BenchmarkKind::Network, CategoryStatus::Completed);
    r.metrics = metrics;
    r.warnings = warnings;
    r.details = details;
    r
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceContext {
    pub name: String,
    pub connection: &'static str,
    pub ipv4: bool,
    pub ipv6: bool,
}

/// Connection type from the adapter name, when it is unambiguous.
pub fn connection_type(name: &str) -> &'static str {
    let n = name.to_ascii_lowercase();
    if n.contains("wi-fi")
        || n.contains("wifi")
        || n.contains("wlan")
        || n.contains("wireless")
        || n.starts_with("wl")
    {
        "Wi-Fi"
    } else if n.contains("ethernet") || n.starts_with("eth") || n.starts_with("en") {
        "Ethernet"
    } else {
        "Unknown"
    }
}

const TUNNEL_HINTS: [&str; 14] = [
    "vpn",
    "tun",
    "tap",
    "wg",
    "wireguard",
    "tailscale",
    "zerotier",
    "nordlynx",
    "openvpn",
    "proton",
    "mullvad",
    "ppp",
    "utun",
    "ipsec",
];

/// Adapters whose names indicate a VPN or tunnel and that are carrying
/// traffic or have an address.
pub fn tunnel_interfaces(networks: &[NetworkInterfaceSnapshot]) -> Vec<String> {
    networks
        .iter()
        .filter(|n| {
            let name = n.name.to_ascii_lowercase();
            let tokens: Vec<&str> = name
                .split(|c: char| !c.is_ascii_alphanumeric())
                .filter(|t| !t.is_empty())
                .collect();
            let hinted = TUNNEL_HINTS.iter().any(|h| {
                if h.len() <= 3 {
                    // Short hints must start a token ("tun0", "wg-home"),
                    // not appear inside words like "Ethernet".
                    tokens.iter().any(|t| t.starts_with(h))
                } else {
                    name.contains(h)
                }
            });
            let active = !n.ipv4.is_empty() || n.rx_bytes_per_sec + n.tx_bytes_per_sec > 0.0;
            hinted && active
        })
        .map(|n| n.name.clone())
        .collect()
}

pub fn interface_context(networks: &[NetworkInterfaceSnapshot]) -> Option<InterfaceContext> {
    let primary = networks.iter().find(|n| n.is_primary)?;
    Some(InterfaceContext {
        name: primary.name.clone(),
        connection: connection_type(&primary.name),
        ipv4: !primary.ipv4.is_empty(),
        ipv6: !primary.ipv6.is_empty(),
    })
}

/// Rough duration for the pre-run summary.
pub fn estimate_secs(profile: NetworkDataProfile) -> u32 {
    match profile {
        NetworkDataProfile::DataSaver => 20,
        NetworkDataProfile::Standard => 40,
        NetworkDataProfile::Maximum => 90,
    }
}

/// How long to wait for the WebView provider before giving up.
fn provider_timeout(profile: NetworkDataProfile) -> Duration {
    Duration::from_secs(match profile {
        NetworkDataProfile::DataSaver => 120,
        NetworkDataProfile::Standard => 240,
        NetworkDataProfile::Maximum => 480,
    })
}

/// A source of internet measurements. Implementations: the Cloudflare
/// provider running in the WebView, and test doubles.
pub trait NetworkProvider: Send {
    fn name(&self) -> &'static str;
    /// Runs `plan` and returns raw samples (validated by the caller).
    fn measure(
        &mut self,
        rc: &mut RunContext<'_, '_>,
        plan: &[NetworkMeasurementSpec],
        max_bytes: u64,
    ) -> AppResult<NetworkSubmission>;
}

/// Drives the `@cloudflare/speedtest` package in the WebView.
pub struct WebViewCloudflareProvider {
    pub inbox: Arc<NetworkInbox>,
}

impl NetworkProvider for WebViewCloudflareProvider {
    fn name(&self) -> &'static str {
        "Cloudflare Speed Test (@cloudflare/speedtest 1.15.0)"
    }

    fn measure(
        &mut self,
        rc: &mut RunContext<'_, '_>,
        plan: &[NetworkMeasurementSpec],
        max_bytes: u64,
    ) -> AppResult<NetworkSubmission> {
        let (token, rx) = self.inbox.open();
        // Always release the slot, whatever happens below.
        struct Close<'a>(&'a NetworkInbox);
        impl Drop for Close<'_> {
            fn drop(&mut self) {
                self.0.close();
            }
        }
        let _close = Close(&self.inbox);

        let run_id = rc.reporter.run_id().to_owned();
        rc.reporter.event(&BenchmarkEvent::NetworkProviderRequest {
            run_id,
            token,
            max_bytes,
            share_results: rc.request.network_share_results,
            measurements: plan.to_vec(),
        });
        let deadline = Instant::now() + provider_timeout(rc.request.network_data_profile);
        loop {
            rc.cancel.check()?;
            if Instant::now() >= deadline {
                return Err(AppError::NetworkTimeout(
                    "The internet test did not finish in time.".into(),
                ));
            }
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(InboxMessage::Progress(p)) => {
                    rc.reporter.progress(p.fraction, p.label.clone());
                    for v in p.live {
                        let unit = if v.series.contains("latency") {
                            MetricUnit::Milliseconds
                        } else {
                            MetricUnit::Mbps
                        };
                        rc.reporter.live(&v.series, v.value, unit);
                    }
                }
                Ok(InboxMessage::Done(s)) => return Ok(*s),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(AppError::ProviderUnavailable(
                        "The internet test provider stopped unexpectedly.".into(),
                    ));
                }
            }
        }
    }
}

pub struct NetworkBenchmark {
    pub provider: Box<dyn NetworkProvider>,
}

impl CategoryBenchmark for NetworkBenchmark {
    fn kind(&self) -> BenchmarkKind {
        BenchmarkKind::Network
    }

    fn run(&mut self, rc: &mut RunContext<'_, '_>) -> AppResult<CategoryResult> {
        rc.reporter
            .phase(BenchmarkPhase::Running, "Measuring internet connection");
        let networks = rc
            .probe
            .latest()
            .map(|t| t.networks.clone())
            .unwrap_or_default();
        let profile = rc.request.network_data_profile;
        let steps = plan(profile);
        let max_bytes = plan_bytes(&steps).min(profile.max_bytes());
        let started = Instant::now();
        let submission = self.provider.measure(rc, &steps, max_bytes)?;
        if let Some(e) = &submission.error {
            return Err(classify_provider_error(e));
        }
        validate(&submission, max_bytes)?;
        if submission.download.is_empty() && submission.idle_latency_ms.is_empty() {
            return Err(AppError::NetworkUnavailable(
                "No internet measurements could be made. Check your connection.".into(),
            ));
        }
        let mut result = analyze(&submission);
        result
            .details
            .push(Detail::new("provider", "Provider", self.provider.name()));
        if let Some(i) = interface_context(&networks) {
            result
                .details
                .push(Detail::new("hardware", "Network interface", i.name.clone()));
            result
                .details
                .push(Detail::new("connection", "Connection type", i.connection));
            let families = match (i.ipv4, i.ipv6) {
                (true, true) => "IPv4 and IPv6",
                (true, false) => "IPv4",
                (false, true) => "IPv6",
                (false, false) => "Unknown",
            };
            result
                .details
                .push(Detail::new("ipFamilies", "Address families", families));
        }
        let tunnels = tunnel_interfaces(&networks);
        if !tunnels.is_empty() {
            result.warnings.push(BenchmarkWarning::new(
                WarningCode::VpnDetected,
                format!(
                    "A VPN or network tunnel may affect these results ({}).",
                    tunnels.join(", ")
                ),
            ));
        }
        result.details.push(Detail::new(
            "duration",
            "Test duration",
            format!("{:.0} s", started.elapsed().as_secs_f64()),
        ));
        result.details.push(Detail::new(
            "methodology",
            "Method",
            "Bandwidth: p90 of transfers ≥ 250 ms. Latency: median round trip. Jitter: mean \
             absolute difference of consecutive round trips.",
        ));
        Ok(result)
    }
}

fn classify_provider_error(message: &str) -> AppError {
    let m = message.to_ascii_lowercase();
    if m.contains("timing") || m.contains("resource") {
        AppError::ProviderUnavailable(
            "The internet test provider cannot measure accurately in this window \
             (resource timing is unavailable)."
                .into(),
        )
    } else if m.contains("timeout") || m.contains("timed out") {
        AppError::NetworkTimeout("The internet test timed out.".into())
    } else {
        AppError::NetworkUnavailable(
            "The internet test could not reach the test server. Check your connection.".into(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmarks::models::PacketLossSample;

    fn t(bytes: f64, ms: f64) -> TransferSample {
        TransferSample {
            bytes,
            duration_ms: ms,
        }
    }

    fn submission() -> NetworkSubmission {
        NetworkSubmission {
            download: vec![
                t(1e5, 20.0),  // 40 Mbps, latency-limited (excluded from headline)
                t(1e7, 400.0), // 200
                t(1e7, 380.0), // 210.5
                t(2.5e7, 900.0),
            ],
            upload: vec![t(1e7, 1_000.0), t(1e7, 950.0)],
            idle_latency_ms: (0..20).map(|i| 18.0 + f64::from(i % 3)).collect(),
            download_loaded_latency_ms: vec![30.0, 32.0, 31.0],
            upload_loaded_latency_ms: vec![70.0, 74.0, 78.0],
            packet_loss: None,
            error: None,
            provider_version: "1.15.0".into(),
        }
    }

    #[test]
    fn mbps_is_bits_not_bytes() {
        // 10 MB in 1 s = 80 Mbps (not 10).
        assert!((mbps(1e7, 1_000.0).unwrap() - 80.0).abs() < 1e-9);
        assert!(mbps(0.0, 10.0).is_none());
        assert!(mbps(10.0, 0.0).is_none());
    }

    #[test]
    fn plans_respect_profile_limits_and_escalate() {
        for p in [
            NetworkDataProfile::DataSaver,
            NetworkDataProfile::Standard,
            NetworkDataProfile::Maximum,
        ] {
            let steps = plan(p);
            assert!(plan_bytes(&steps) <= p.max_bytes(), "{p:?}");
            assert!(
                steps
                    .iter()
                    .filter(|s| s.kind == NetworkMeasurementKind::Latency)
                    .map(|s| s.count)
                    .sum::<u32>()
                    >= 20
            );
            assert!(
                steps
                    .iter()
                    .any(|s| s.kind == NetworkMeasurementKind::Upload)
            );
        }
        assert!(
            plan_bytes(&plan(NetworkDataProfile::Maximum))
                > plan_bytes(&plan(NetworkDataProfile::Standard))
        );
    }

    #[test]
    fn analysis_computes_bandwidth_latency_jitter_and_loaded_latency() {
        let r = analyze(&submission());
        let down = r.metric("net.download").unwrap();
        // Long transfers only: 200, 210.5, 222.2 → p90 interpolates near the top.
        assert!(down.value > 210.0 && down.value < 223.0, "{}", down.value);
        assert_eq!(down.samples.len(), 3);
        let up = r.metric("net.upload").unwrap();
        assert!((up.value - 84.0).abs() < 1.0);
        let idle = r.metric("net.latency.idle").unwrap();
        assert!((idle.value - 19.0).abs() < 1e-9);
        assert!(r.metric("net.jitter.idle").unwrap().value > 0.0);
        assert!(r.metric("net.latency.upload").unwrap().value > 70.0);
        assert!(
            r.warnings
                .iter()
                .any(|w| w.code == WarningCode::LatencyIncreasedUnderLoad)
        );
        assert!(r.metric("net.packet_loss").is_none());
        assert!(r.details.iter().any(|d| d.value.contains("unavailable")));
        assert!((r.metric("net.bytes_transferred").unwrap().value - 6.51e7).abs() < 1.0);
    }

    #[test]
    fn packet_loss_only_when_measured() {
        let mut s = submission();
        s.packet_loss = Some(PacketLossSample {
            sent: 1000,
            lost: 5,
        });
        let r = analyze(&s);
        assert!((r.metric("net.packet_loss").unwrap().value - 0.5).abs() < 1e-9);
    }

    #[test]
    fn few_latency_samples_are_flagged() {
        let mut s = submission();
        s.idle_latency_ms.truncate(5);
        let r = analyze(&s);
        assert!(
            r.warnings
                .iter()
                .any(|w| w.code == WarningCode::InsufficientSamples)
        );
    }

    #[test]
    fn rejects_implausible_or_oversized_submissions() {
        let max = 200_000_000;
        assert!(validate(&submission(), max).is_ok());
        let mut s = submission();
        s.idle_latency_ms.push(f64::NAN);
        assert!(validate(&s, max).is_err());
        let mut s = submission();
        s.download.push(t(-1.0, 10.0));
        assert!(validate(&s, max).is_err());
        let mut s = submission();
        s.download = vec![t(9e8, 1_000.0)];
        assert!(validate(&s, 25_000_000).is_err(), "exceeds data profile");
        let mut s = submission();
        s.idle_latency_ms = vec![10.0; MAX_SAMPLES + 1];
        assert!(validate(&s, max).is_err());
        let mut s = submission();
        s.packet_loss = Some(PacketLossSample { sent: 10, lost: 11 });
        assert!(validate(&s, max).is_err());
    }

    fn iface(name: &str, ipv4: bool, primary: bool) -> NetworkInterfaceSnapshot {
        NetworkInterfaceSnapshot {
            name: name.into(),
            rx_bytes_per_sec: 0.0,
            tx_bytes_per_sec: 0.0,
            total_rx_bytes: 0,
            total_tx_bytes: 0,
            ipv4: if ipv4 {
                vec!["10.0.0.2".into()]
            } else {
                Vec::new()
            },
            ipv6: Vec::new(),
            mac_address: None,
            is_primary: primary,
            kind: crate::models::InterfaceKind::Unknown,
        }
    }

    #[test]
    fn detects_active_tunnels_by_name() {
        let nets = vec![
            iface("Ethernet", true, true),
            iface("Wi-Fi", false, false),
            iface("NordLynx", true, false),
            iface("tun0", true, false),
            iface("wg-home", false, false), // no address, no traffic → ignored
            iface("Tailscale", true, false),
        ];
        assert_eq!(
            tunnel_interfaces(&nets),
            vec!["NordLynx", "tun0", "Tailscale"]
        );
        assert!(tunnel_interfaces(&[iface("Ethernet 2", true, true)]).is_empty());
    }

    #[test]
    fn classifies_connection_types() {
        assert_eq!(connection_type("Wi-Fi"), "Wi-Fi");
        assert_eq!(connection_type("wlp2s0"), "Wi-Fi");
        assert_eq!(connection_type("Ethernet"), "Ethernet");
        assert_eq!(connection_type("enp3s0"), "Ethernet");
        assert_eq!(connection_type("Cellular"), "Unknown");
        let ctx = interface_context(&[iface("Wi-Fi", true, true)]).unwrap();
        assert_eq!(ctx.connection, "Wi-Fi");
        assert!(ctx.ipv4 && !ctx.ipv6);
    }

    #[test]
    fn provider_errors_map_to_user_facing_kinds() {
        use crate::errors::ErrorKind;
        assert_eq!(
            classify_provider_error("PerformanceResourceTiming unavailable").kind(),
            ErrorKind::ProviderUnavailable
        );
        assert_eq!(
            classify_provider_error("Request timed out").kind(),
            ErrorKind::NetworkTimeout
        );
        assert_eq!(
            classify_provider_error("Failed to fetch").kind(),
            ErrorKind::NetworkUnavailable
        );
    }
}

#[cfg(test)]
mod provider_tests {
    use super::*;
    use crate::benchmarks::cancellation::{CancelReason, CancelToken};
    use crate::benchmarks::coordinator_test_request;
    use crate::benchmarks::environment::tests::FakeProbe;
    use crate::benchmarks::models::{BenchmarkRequest, NetworkProgress, TransferSample};
    use crate::benchmarks::progress::{ProgressSink, Reporter};
    use std::sync::Mutex;

    /// Plays the WebView: answers a provider request through the inbox.
    struct FakeWebView {
        inbox: Arc<NetworkInbox>,
        respond: bool,
        seen: Mutex<Vec<NetworkMeasurementSpec>>,
    }

    impl ProgressSink for FakeWebView {
        fn send(&self, event: &BenchmarkEvent) -> bool {
            if let BenchmarkEvent::NetworkProviderRequest {
                token,
                measurements,
                share_results,
                ..
            } = event
            {
                assert!(!share_results, "sharing is opt-in");
                *self.seen.lock().unwrap() = measurements.clone();
                if self.respond {
                    let inbox = Arc::clone(&self.inbox);
                    let token = token.clone();
                    std::thread::spawn(move || {
                        inbox
                            .progress(
                                &token,
                                NetworkProgress {
                                    fraction: 0.5,
                                    label: "Download".into(),
                                    live: vec![super::super::models::NetworkLiveValue {
                                        series: "net.download".into(),
                                        value: 100.0,
                                    }],
                                },
                            )
                            .unwrap();
                        let s = NetworkSubmission {
                            download: vec![TransferSample {
                                bytes: 1e7,
                                duration_ms: 500.0,
                            }],
                            upload: vec![TransferSample {
                                bytes: 1e6,
                                duration_ms: 300.0,
                            }],
                            idle_latency_ms: vec![20.0; 25],
                            provider_version: "test".into(),
                            ..Default::default()
                        };
                        inbox.submit(&token, s).unwrap();
                    });
                }
            }
            true
        }
    }

    fn run(respond: bool, cancel_after: Option<Duration>) -> AppResult<CategoryResult> {
        let inbox = Arc::new(NetworkInbox::default());
        let sink = FakeWebView {
            inbox: Arc::clone(&inbox),
            respond,
            seen: Mutex::new(Vec::new()),
        };
        let probe = FakeProbe::new(1.0);
        let cancel = Arc::new(CancelToken::new(Duration::from_secs(60)));
        if let Some(after) = cancel_after {
            let c = Arc::clone(&cancel);
            std::thread::spawn(move || {
                std::thread::sleep(after);
                c.cancel(CancelReason::User);
            });
        }
        let request = BenchmarkRequest {
            kinds: vec![BenchmarkKind::Network],
            ..coordinator_test_request()
        };
        let mut reporter = Reporter::new(&sink, "run", Instant::now());
        let mut ctx = RunContext {
            request: &request,
            cancel: &cancel,
            reporter: &mut reporter,
            probe: &*probe,
        };
        let mut bench = NetworkBenchmark {
            provider: Box::new(WebViewCloudflareProvider {
                inbox: Arc::clone(&inbox),
            }),
        };
        let result = bench.run(&mut ctx);
        assert!(!sink.seen.lock().unwrap().is_empty(), "plan was sent");
        // The inbox is always closed afterwards.
        assert!(inbox.submit("any", NetworkSubmission::default()).is_err());
        result
    }

    #[test]
    fn webview_handshake_produces_validated_results() {
        let r = run(true, None).unwrap();
        assert!((r.metric("net.download").unwrap().value - 160.0).abs() < 1e-6);
        assert!(r.details.iter().any(|d| d.key == "provider"));
    }

    #[test]
    fn cancellation_stops_waiting_for_the_webview() {
        let started = Instant::now();
        let err = run(false, Some(Duration::from_millis(200))).unwrap_err();
        assert!(matches!(err, AppError::BenchmarkCancelled));
        assert!(started.elapsed() < Duration::from_secs(3));
    }
}
