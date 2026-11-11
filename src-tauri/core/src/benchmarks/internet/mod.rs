//! Internet benchmark: latency, jitter, download and upload throughput and
//! latency under load, against one fixed provider (see [`provider`]).
//!
//! Only generated test bytes leave the machine. Throughput is bytes actually
//! moved divided by time, sampled in short windows after a ramp-up that is
//! excluded. There is no score (the path to the server dominates the result)
//! and no packet-loss figure (HTTP cannot measure it).

pub mod provider;
pub mod route;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use super::cancel::CancelToken;
use super::cpu::runner::Observer;
use super::model::{
    BenchmarkProfile, BenchmarkState, InternetPlan, InternetResult, LatencyStats, LiveReading,
    NetworkPath, TransferResult,
};
use super::{BenchmarkError, stats};
use provider::{NetError, Provider, Session};

/// How often a wait on network work checks for cancellation.
const CANCEL_POLL: Duration = Duration::from_millis(50);
/// How long a finished phase waits for its connections to close.
const WORKER_GRACE: Duration = Duration::from_millis(500);

#[derive(Debug, Clone)]
pub struct InternetConfig {
    pub idle_pings: usize,
    pub ping_gap: Duration,
    pub down_connections: u32,
    pub up_connections: u32,
    /// Excluded from the result: slow start and connection set-up.
    pub ramp: Duration,
    /// Measured time after the ramp.
    pub window: Duration,
    pub sample_interval: Duration,
    pub loaded_ping_gap: Duration,
    pub chunk_down: u64,
    pub chunk_up: u64,
    /// A direction stops once this much has moved, however fast the link is.
    pub max_down: u64,
    pub max_up: u64,
}

const MB: u64 = 1_000_000;

impl InternetConfig {
    pub fn for_profile(profile: BenchmarkProfile) -> Self {
        let (pings, conns, ramp, window, max_down, max_up) = match profile {
            BenchmarkProfile::Quick => (10, 4, 1, 5, 100 * MB, 30 * MB),
            BenchmarkProfile::Standard => (20, 6, 2, 8, 250 * MB, 80 * MB),
            BenchmarkProfile::Extended => (40, 8, 2, 15, 600 * MB, 200 * MB),
        };
        Self {
            idle_pings: pings,
            ping_gap: Duration::from_millis(100),
            down_connections: conns,
            up_connections: conns.min(4),
            ramp: Duration::from_secs(ramp),
            window: Duration::from_secs(window),
            sample_interval: Duration::from_millis(250),
            loaded_ping_gap: Duration::from_millis(500),
            // Sizes Cloudflare's own speed test uses (an arbitrary size such as
            // 16 000 000 bytes was answered with HTTP 403). Large chunks keep
            // the request rate low: it was rate-limited (429) at 10 MB.
            chunk_down: 25 * MB,
            chunk_up: 10 * MB,
            max_down,
            max_up,
        }
    }

    fn seconds(&self) -> u32 {
        let t = (self.ramp + self.window).as_secs() as u32;
        self.idle_pings as u32 / 8 + 2 * t + 4
    }

    #[cfg(test)]
    pub fn test() -> Self {
        Self {
            idle_pings: 5,
            ping_gap: Duration::from_millis(10),
            down_connections: 2,
            up_connections: 2,
            ramp: Duration::from_millis(300),
            window: Duration::from_millis(1500),
            sample_interval: Duration::from_millis(100),
            loaded_ping_gap: Duration::from_millis(100),
            chunk_down: 4 * MB,
            chunk_up: 2 * MB,
            max_down: 400 * MB,
            max_up: 400 * MB,
        }
    }
}

/// What a run would send and receive, for the confirmation text.
pub fn plan(provider: &dyn Provider, profile: BenchmarkProfile) -> InternetPlan {
    let c = InternetConfig::for_profile(profile);
    InternetPlan {
        provider: provider.name().into(),
        host: provider.host().into(),
        max_download_bytes: c.max_down,
        max_upload_bytes: c.max_up,
        estimated_seconds: c.seconds(),
    }
}

fn fail(host: &str, e: &NetError, cancel: &CancelToken) -> BenchmarkError {
    if cancel.is_cancelled() {
        BenchmarkError::Cancelled
    } else {
        BenchmarkError::Failed(e.message(host))
    }
}

fn latency_stats(ms: &[f64]) -> Option<LatencyStats> {
    let s = stats::summarize(ms)?;
    let jitter = if ms.len() > 1 {
        ms.windows(2).map(|w| (w[1] - w[0]).abs()).sum::<f64>() / (ms.len() - 1) as f64
    } else {
        0.0
    };
    Some(LatencyStats {
        samples: ms.len() as u32,
        min_ms: s.min,
        median_ms: s.median,
        p95_ms: s.p95.unwrap_or(s.max),
        max_ms: s.max,
        jitter_ms: jitter,
    })
}

/// Runs `f` on its own thread and waits for it, but stops waiting once
/// `cancel` fires. Shutting a socket down does not wake a blocked read on
/// every platform (Windows), so such a thread is left to reach its timeout.
fn unless_cancelled<T: Send + 'static>(
    cancel: &CancelToken,
    f: impl FnOnce() -> T + Send + 'static,
) -> Result<T, BenchmarkError> {
    let (tx, rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    loop {
        match rx.recv_timeout(CANCEL_POLL) {
            Ok(value) => return Ok(value),
            Err(RecvTimeoutError::Timeout) if cancel.is_cancelled() => {
                return Err(BenchmarkError::Cancelled);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                std::panic::resume_unwind(worker.join().unwrap_err())
            }
        }
    }
}

/// The session, handed back for the next request, and the round trip.
type Pinged = (Box<dyn Session>, Result<Duration, NetError>);

fn ping(session: Box<dyn Session>, cancel: &CancelToken) -> Result<Pinged, BenchmarkError> {
    unless_cancelled(cancel, move || {
        let mut session = session;
        let rtt = session.ping();
        (session, rtt)
    })
}

/// Round trips on an already-open connection. The first request pays for the
/// connection set-up and is discarded.
fn idle_latency(
    provider: &dyn Provider,
    cfg: &InternetConfig,
    cancel: &CancelToken,
    observer: &dyn Observer,
) -> Result<(Vec<f64>, Option<String>), BenchmarkError> {
    let host = provider.host();
    if cancel.is_cancelled() {
        return Err(BenchmarkError::Cancelled);
    }
    let session = provider
        .session(cancel)
        .map_err(|e| fail(host, &e, cancel))?;
    let (mut session, first) = ping(session, cancel)?;
    first.map_err(|e| fail(host, &e, cancel))?;
    let mut rtts = Vec::with_capacity(cfg.idle_pings);
    for i in 0..cfg.idle_pings {
        if cancel.is_cancelled() {
            return Err(BenchmarkError::Cancelled);
        }
        let (next, rtt) = ping(session, cancel)?;
        session = next;
        let rtt = rtt.map_err(|e| fail(host, &e, cancel))?;
        rtts.push(rtt.as_secs_f64() * 1000.0);
        observer.stage(
            BenchmarkState::Running,
            "Latency",
            0.1 * (i + 1) as f32 / cfg.idle_pings as f32,
        );
        std::thread::sleep(cfg.ping_gap);
    }
    Ok((rtts, session.server_location()))
}

/// State one transfer phase shares with its connection threads, which may
/// outlive the phase (see [`unless_cancelled`]).
struct Phase {
    total: AtomicU64,
    stop: AtomicBool,
    failure: Mutex<Option<NetError>>,
    pings: Mutex<Vec<(f64, f64)>>,
    cancel: CancelToken,
}

impl Phase {
    fn record(&self, e: NetError) {
        let mut failure = self.failure.lock().unwrap_or_else(PoisonError::into_inner);
        let interrupted =
            self.cancel.is_cancelled() && matches!(e, NetError::Unreachable(_) | NetError::Timeout);
        if !interrupted {
            failure.get_or_insert(e);
        }
        self.stop.store(true, Ordering::Relaxed);
        drop(failure);
        self.cancel.cancel();
    }
}

/// Saturates the link in one direction and samples throughput. `span` is the
/// share of the progress bar this phase covers.
fn transfer(
    provider: &dyn Provider,
    cfg: &InternetConfig,
    upload: bool,
    cancel: &CancelToken,
    session_bytes: &Arc<AtomicU64>,
    observer: &dyn Observer,
    span: (f32, f32),
) -> Result<TransferResult, BenchmarkError> {
    let host = provider.host();
    let (conns, cap, chunk) = if upload {
        (cfg.up_connections, cfg.max_up, cfg.chunk_up)
    } else {
        (cfg.down_connections, cfg.max_down, cfg.chunk_down)
    };
    let label = if upload { "Upload" } else { "Download" };
    let phase = Arc::new(Phase {
        total: AtomicU64::new(0),
        stop: AtomicBool::new(false),
        failure: Mutex::new(None),
        pings: Mutex::new(Vec::new()),
        cancel: CancelToken::new(),
    });
    let phase_cancel = phase.cancel.clone();
    cancel.on_cancel(move || phase_cancel.cancel());
    let mut samples: Vec<(f64, f64)> = Vec::new();
    let start = Instant::now();
    let planned = (cfg.ramp + cfg.window).as_secs_f64();
    // Each thread holds a sender; `finished` disconnects once all have exited.
    let (running, finished) = mpsc::channel::<()>();

    for _ in 0..conns {
        let session = provider.session(&phase.cancel);
        let (phase, cancel, session_bytes, running) = (
            Arc::clone(&phase),
            cancel.clone(),
            Arc::clone(session_bytes),
            running.clone(),
        );
        std::thread::spawn(move || {
            let _running = running;
            let mut session = match session {
                Ok(s) => s,
                Err(e) => return phase.record(e),
            };
            while !phase.stop.load(Ordering::Relaxed) {
                let mut count = |n: usize| {
                    let t = phase.total.fetch_add(n as u64, Ordering::Relaxed) + n as u64;
                    session_bytes.fetch_add(n as u64, Ordering::Relaxed);
                    if t >= cap || cancel.is_cancelled() {
                        phase.stop.store(true, Ordering::Relaxed);
                    }
                    !phase.stop.load(Ordering::Relaxed)
                };
                let r = if upload {
                    session.upload(chunk, &mut count)
                } else {
                    session.download(chunk, &mut count)
                };
                if let Err(e) = r {
                    return phase.record(e);
                }
            }
        });
    }
    // Latency while the link is busy, on its own connection.
    if let Ok(mut session) = provider.session(&phase.cancel) {
        let (phase, gap, running) = (Arc::clone(&phase), cfg.loaded_ping_gap, running.clone());
        std::thread::spawn(move || {
            let _running = running;
            if session.ping().is_err() {
                return;
            }
            while !phase.stop.load(Ordering::Relaxed) {
                if let Ok(rtt) = session.ping() {
                    phase
                        .pings
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push((start.elapsed().as_secs_f64(), rtt.as_secs_f64() * 1000.0));
                }
                std::thread::sleep(gap);
            }
        });
    }
    drop(running);

    let (mut last_t, mut last_bytes) = (0.0f64, 0u64);
    loop {
        std::thread::sleep(cfg.sample_interval);
        let t = start.elapsed().as_secs_f64();
        let bytes = phase.total.load(Ordering::Relaxed);
        if t > last_t {
            let mbps = (bytes - last_bytes) as f64 * 8.0 / 1e6 / (t - last_t);
            samples.push((t, mbps));
            observer.live(Some(LiveReading {
                label: label.into(),
                value: mbps,
                unit: "Mbit/s".into(),
            }));
        }
        (last_t, last_bytes) = (t, bytes);
        observer.stage(
            BenchmarkState::Running,
            label,
            span.0 + (span.1 - span.0) * (t / planned).min(1.0) as f32,
        );
        if t >= planned || phase.stop.load(Ordering::Relaxed) || cancel.is_cancelled() {
            break;
        }
    }
    phase.stop.store(true, Ordering::Relaxed);
    phase.cancel.cancel();

    if !cancel.is_cancelled() {
        let _ = finished.recv_timeout(WORKER_GRACE);
    }
    if cancel.is_cancelled() {
        return Err(BenchmarkError::Cancelled);
    }
    if let Some(e) = phase
        .failure
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()
    {
        return Err(fail(host, &e, cancel));
    }
    let elapsed = samples.last().map_or(0.0, |s| s.0);
    // A very fast link reaches the data cap before the ramp-up is over; then
    // the first third is excluded instead.
    let ramp = if elapsed >= cfg.ramp.as_secs_f64() + 3.0 * cfg.sample_interval.as_secs_f64() {
        cfg.ramp.as_secs_f64()
    } else {
        elapsed / 3.0
    };
    let mbps: Vec<f64> = samples
        .iter()
        .filter(|(t, _)| *t > ramp)
        .map(|(_, v)| *v)
        .collect();
    let s = stats::summarize(&mbps)
        .filter(|s| s.median > 0.0)
        .ok_or_else(|| {
            BenchmarkError::Failed(format!(
                "The {} did not run long enough to measure. Check the connection and try again.",
                label.to_lowercase()
            ))
        })?;
    let loaded: Vec<f64> =
        std::mem::take(&mut *phase.pings.lock().unwrap_or_else(PoisonError::into_inner))
            .into_iter()
            .filter(|(t, _)| *t > ramp)
            .map(|(_, ms)| ms)
            .collect();
    Ok(TransferResult {
        median_mbps: s.median,
        mean_mbps: s.mean,
        peak_mbps: s.max,
        samples: mbps,
        bytes: phase.total.load(Ordering::Relaxed),
        seconds: elapsed,
        connections: conns,
        loaded_latency: latency_stats(&loaded),
    })
}

pub fn run_internet(
    provider: &dyn Provider,
    cfg: &InternetConfig,
    path: NetworkPath,
    cancel: &CancelToken,
    session_bytes: &Arc<AtomicU64>,
    observer: &dyn Observer,
) -> Result<InternetResult, BenchmarkError> {
    observer.stage(BenchmarkState::Preflight, "Connecting", 0.0);
    let (rtts, location) = idle_latency(provider, cfg, cancel, observer)?;
    let download = transfer(
        provider,
        cfg,
        false,
        cancel,
        session_bytes,
        observer,
        (0.1, 0.55),
    )?;
    let upload = transfer(
        provider,
        cfg,
        true,
        cancel,
        session_bytes,
        observer,
        (0.55, 1.0),
    )?;
    let mut notes = vec![
        "Throughput is bytes actually moved, sampled in short windows after the first seconds of ramp-up.".to_owned(),
        "Packet loss is not measured: HTTP cannot tell a lost packet from a retransmission.".to_owned(),
        "Upload counts bytes handed to the connection, so it can read slightly high on a very short run.".to_owned(),
    ];
    if download.samples.len() < 3 || upload.samples.len() < 3 {
        notes.push("A transfer finished almost at once, so that measurement is coarse.".into());
    }
    if path.vpn_suspected {
        notes.push(
            "A VPN or tunnel looks active; the results include its overhead and route.".into(),
        );
    }
    if download.bytes >= cfg.max_down {
        notes.push("The download reached its data limit; the link may be faster than measured over a longer test.".into());
    }
    if upload.bytes >= cfg.max_up {
        notes.push("The upload reached its data limit; the link may be faster than measured over a longer test.".into());
    }
    Ok(InternetResult {
        provider: provider.name().into(),
        host: provider.host().into(),
        server_location: location,
        path,
        idle_latency: latency_stats(&rtts),
        bytes_downloaded: download.bytes,
        bytes_uploaded: upload.bytes,
        download: Some(download),
        upload: Some(upload),
        packet_loss_percent: None,
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::provider::Cloudflare;
    use super::provider::test_server::{Mode, start};
    use super::*;
    use std::sync::atomic::AtomicUsize;

    struct Quiet(AtomicUsize);
    impl Observer for Quiet {
        fn stage(&self, _: BenchmarkState, _: &str, f: f32) {
            assert!((0.0..=1.0).contains(&f), "{f}");
            self.0.fetch_add(1, Ordering::Relaxed);
        }
        fn live(&self, _: Option<LiveReading>) {}
    }

    fn quiet() -> Quiet {
        Quiet(AtomicUsize::new(0))
    }

    fn provider(base: &str) -> Cloudflare {
        Cloudflare::with_base(base, Duration::from_secs(5))
    }

    const PATH: NetworkPath = NetworkPath {
        interface: None,
        kind: None,
        vpn_suspected: false,
    };

    #[test]
    fn throughput_matches_the_rate_the_server_really_delivers() {
        // 2 connections at 2.5 MB/s each = 40 Mbit/s.
        let server = start(Mode::Normal, 2_500_000);
        let out = run_internet(
            &provider(&server.base),
            &InternetConfig::test(),
            PATH,
            &CancelToken::new(),
            &Arc::new(AtomicU64::new(0)),
            &quiet(),
        )
        .expect("runs");
        let d = out.download.expect("download");
        assert!(
            (34.0..46.0).contains(&d.median_mbps),
            "download {} Mbit/s, expected about 40",
            d.median_mbps
        );
        assert_eq!(d.connections, 2);
        assert!(d.samples.len() >= 5 && d.peak_mbps >= d.median_mbps);
        assert_eq!(out.bytes_downloaded, d.bytes);
        assert_eq!(out.server_location.as_deref(), Some("TST"));
        let idle = out.idle_latency.expect("latency");
        assert_eq!(idle.samples, 5);
        assert!(idle.min_ms <= idle.median_ms && idle.median_ms <= idle.max_ms);
        assert!(idle.jitter_ms >= 0.0);
        assert!(d.loaded_latency.is_some(), "latency under load is measured");
        assert_eq!(out.packet_loss_percent, None);
    }

    #[test]
    fn upload_bytes_counted_match_what_the_server_received() {
        let server = start(Mode::Normal, 0);
        let mut cfg = InternetConfig::test();
        cfg.max_down = 20 * MB;
        cfg.max_up = 20 * MB;
        let out = run_internet(
            &provider(&server.base),
            &cfg,
            PATH,
            &CancelToken::new(),
            &Arc::new(AtomicU64::new(0)),
            &quiet(),
        )
        .expect("runs");
        let up = out.upload.expect("upload");
        assert!(up.median_mbps > 0.0);
        std::thread::sleep(Duration::from_millis(200));
        let got = server.uploaded.load(Ordering::Relaxed);
        // We count what we handed to the connection; the server sees all of
        // it, give or take what is still in flight when the run stops.
        let (counted, slack) = (up.bytes as i64, 4 * MB as i64);
        assert!(
            (got as i64 - counted).abs() <= slack,
            "counted {counted}, server got {got}"
        );
        assert!(out.notes.iter().any(|n| n.contains("data limit")));
        assert!(up.bytes < 40 * MB, "stopped near the cap: {}", up.bytes);
    }

    #[test]
    fn a_short_body_is_reported_not_measured() {
        let server = start(Mode::ShortBody, 0);
        let e = run_internet(
            &provider(&server.base),
            &InternetConfig::test(),
            PATH,
            &CancelToken::new(),
            &Arc::new(AtomicU64::new(0)),
            &quiet(),
        )
        .expect_err("fails");
        assert!(e.to_string().contains("Peer disconnected"), "{e}");
    }

    #[test]
    fn a_non_http_server_and_an_error_status_are_reported() {
        for (mode, text) in [
            (Mode::Garbage, "Could not reach"),
            (Mode::Unavailable, "unexpected response"),
        ] {
            let server = start(mode, 0);
            let e = run_internet(
                &provider(&server.base),
                &InternetConfig::test(),
                PATH,
                &CancelToken::new(),
                &Arc::new(AtomicU64::new(0)),
                &quiet(),
            )
            .expect_err("fails");
            assert!(e.to_string().contains(text), "{e}");
        }
    }

    #[test]
    fn a_stalled_server_times_out_with_a_plain_message() {
        let server = start(Mode::Stall, 0);
        let started = Instant::now();
        let e = run_internet(
            &Cloudflare::with_base(&server.base, Duration::from_millis(600)),
            &InternetConfig::test(),
            PATH,
            &CancelToken::new(),
            &Arc::new(AtomicU64::new(0)),
            &quiet(),
        )
        .expect_err("fails");
        assert!(e.to_string().contains("did not respond in time"), "{e}");
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_rate_limited_server_says_when_to_try_again() {
        let server = start(Mode::RateLimited, 0);
        let e = run_internet(
            &provider(&server.base),
            &InternetConfig::test(),
            PATH,
            &CancelToken::new(),
            &Arc::new(AtomicU64::new(0)),
            &quiet(),
        )
        .expect_err("fails");
        assert!(
            e.to_string().contains("HTTP 429") && e.to_string().contains("in about 2 minutes"),
            "{e}"
        );
    }

    #[test]
    fn an_unreachable_server_is_a_clear_error() {
        // A port nothing listens on.
        let e = run_internet(
            &provider("http://127.0.0.1:1"),
            &InternetConfig::test(),
            PATH,
            &CancelToken::new(),
            &Arc::new(AtomicU64::new(0)),
            &quiet(),
        )
        .expect_err("fails");
        assert!(e.to_string().contains("Could not reach"), "{e}");
    }

    #[test]
    fn cancelling_mid_transfer_stops_quickly() {
        let server = start(Mode::Normal, 1_000_000);
        let cancel = CancelToken::new();
        let c2 = cancel.clone();
        let t = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(1200));
            c2.cancel();
        });
        let mut cfg = InternetConfig::test();
        cfg.window = Duration::from_secs(30);
        let started = Instant::now();
        let r = run_internet(
            &provider(&server.base),
            &cfg,
            PATH,
            &cancel,
            &Arc::new(AtomicU64::new(0)),
            &quiet(),
        );
        t.join().unwrap();
        assert!(matches!(r, Err(BenchmarkError::Cancelled)), "{r:?}");
        assert!(
            started.elapsed() < Duration::from_secs(4),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn cancellation_interrupts_stalled_headers_downloads_and_uploads() {
        for mode in [Mode::StallHeaders, Mode::StallDownload, Mode::StallUpload] {
            let server = start(mode, 0);
            let cancel = CancelToken::new();
            let token = cancel.clone();
            let stop = std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(700));
                token.cancel();
            });
            let mut cfg = InternetConfig::test();
            cfg.idle_pings = 1;
            cfg.max_down = 10 * MB;
            cfg.chunk_up = 50 * MB;
            cfg.window = Duration::from_secs(30);
            let started = Instant::now();
            let result = run_internet(
                &provider(&server.base),
                &cfg,
                PATH,
                &cancel,
                &Arc::new(AtomicU64::new(0)),
                &quiet(),
            );
            stop.join().unwrap();
            assert!(
                matches!(result, Err(BenchmarkError::Cancelled)),
                "{result:?}"
            );
            assert!(
                started.elapsed() < Duration::from_secs(3),
                "cancellation took {:?}",
                started.elapsed()
            );
        }
    }

    #[test]
    fn phase_deadline_interrupts_stalled_workers_and_loaded_latency() {
        let server = start(Mode::StallHeaders, 0);
        let mut cfg = InternetConfig::test();
        cfg.ramp = Duration::ZERO;
        cfg.window = Duration::from_millis(300);
        let started = Instant::now();
        let result = transfer(
            &provider(&server.base),
            &cfg,
            false,
            &CancelToken::new(),
            &Arc::new(AtomicU64::new(0)),
            &quiet(),
            (0.0, 1.0),
        );
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("did not run long enough")
        );
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn failed_and_cancelled_transfers_still_consume_the_session_allowance() {
        struct Failing {
            cancel: CancelToken,
            cancelled: bool,
        }
        impl provider::Provider for Failing {
            fn name(&self) -> &str {
                "test"
            }
            fn host(&self) -> &str {
                "local fixture"
            }
            fn session(&self, _: &CancelToken) -> Result<Box<dyn provider::Session>, NetError> {
                Ok(Box::new(Failing {
                    cancel: self.cancel.clone(),
                    cancelled: self.cancelled,
                }))
            }
        }
        impl provider::Session for Failing {
            fn ping(&mut self) -> Result<Duration, NetError> {
                Ok(Duration::from_millis(1))
            }
            fn download(
                &mut self,
                _: u64,
                progress: &mut dyn FnMut(usize) -> bool,
            ) -> Result<(), NetError> {
                progress(65_536);
                if self.cancelled {
                    self.cancel.cancel();
                }
                Err(NetError::BadResponse(
                    "fixture failed after transferring data".into(),
                ))
            }
            fn upload(&mut self, _: u64, _: &mut dyn FnMut(usize) -> bool) -> Result<(), NetError> {
                unreachable!()
            }
        }
        for (cancelled, cap) in [(false, 400 * MB), (false, 65_536), (true, 400 * MB)] {
            let cancel = CancelToken::new();
            let provider = Failing {
                cancel: cancel.clone(),
                cancelled,
            };
            let used = Arc::new(AtomicU64::new(0));
            let mut cfg = InternetConfig::test();
            cfg.down_connections = 1;
            cfg.max_down = cap;
            cfg.idle_pings = 1;
            let result = run_internet(&provider, &cfg, PATH, &cancel, &used, &quiet());
            assert!(result.is_err());
            assert_eq!(matches!(result, Err(BenchmarkError::Cancelled)), cancelled);
            assert_eq!(used.load(Ordering::Relaxed), 65_536);
        }
    }

    #[test]
    fn the_plan_states_the_limits_and_the_fixed_host() {
        let p = plan(&Cloudflare::new(), BenchmarkProfile::Standard);
        assert_eq!(p.host, "speed.cloudflare.com");
        assert_eq!(
            (p.max_download_bytes, p.max_upload_bytes),
            (250 * MB, 80 * MB)
        );
        assert!(p.estimated_seconds > 10);
        let q = plan(&Cloudflare::new(), BenchmarkProfile::Quick);
        assert!(q.max_download_bytes < p.max_download_bytes);
    }

    #[test]
    fn jitter_is_the_mean_change_between_consecutive_round_trips() {
        let s = latency_stats(&[10.0, 12.0, 11.0, 15.0]).unwrap();
        assert!((s.jitter_ms - (2.0 + 1.0 + 4.0) / 3.0).abs() < 1e-9);
        assert_eq!(s.samples, 4);
        assert_eq!(latency_stats(&[]), None);
        assert_eq!(latency_stats(&[7.0]).unwrap().jitter_ms, 0.0);
    }
}
