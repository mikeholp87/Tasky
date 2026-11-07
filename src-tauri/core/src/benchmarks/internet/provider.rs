//! Where the internet benchmark connects. All access goes through
//! [`Provider`], so no URL appears anywhere else, and the only provider is
//! fixed in code: the frontend cannot supply an address.
//!
//! The real provider speaks HTTPS with ordinary certificate verification.
//! Nothing but generated test bytes is ever sent: no files, no system
//! information, no benchmark history.

use crate::benchmarks::CancelToken;
use std::io::Read;
use std::time::{Duration, Instant};

/// Why a request failed, in terms a person can act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetError {
    Timeout,
    /// HTTP 429 or 403: the server is limiting or refusing us. The public
    /// speed-test endpoints limit large transfers per address.
    Limited {
        code: u16,
        retry_after_secs: Option<u64>,
    },
    Unreachable(String),
    BadResponse(String),
}

impl NetError {
    pub fn message(&self, host: &str) -> String {
        match self {
            Self::Timeout => {
                format!("{host} did not respond in time. Check the connection and try again.")
            }
            Self::Limited {
                code,
                retry_after_secs,
            } => {
                let when = match retry_after_secs {
                    Some(s) if *s >= 90 => format!("in about {} minutes", s.div_ceil(60)),
                    Some(s) => format!("in about {s} seconds"),
                    None => "in a few minutes".into(),
                };
                format!(
                    "{host} is limiting requests from this connection right now (HTTP {code}), probably because of earlier tests. Try again {when}."
                )
            }
            Self::Unreachable(why) => {
                format!("Could not reach {host}: {why}. Check the connection.")
            }
            Self::BadResponse(what) => format!(
                "{host} sent an unexpected response ({what}), so the measurement was discarded."
            ),
        }
    }
}

/// One connection's worth of requests (keep-alive within a session).
pub trait Session: Send {
    /// Round-trip time of a tiny request.
    fn ping(&mut self) -> Result<Duration, NetError>;
    /// Downloads `bytes`, calling `progress(n)` for every chunk received. When
    /// `progress` returns `false` the transfer stops early and this returns
    /// `Ok(())`: stopping on purpose is not a failure.
    fn download(
        &mut self,
        bytes: u64,
        progress: &mut dyn FnMut(usize) -> bool,
    ) -> Result<(), NetError>;
    /// Uploads `bytes` of generated data; `progress` as for `download`, but
    /// counting bytes handed to the connection.
    fn upload(
        &mut self,
        bytes: u64,
        progress: &mut dyn FnMut(usize) -> bool,
    ) -> Result<(), NetError>;
    /// The server's data-centre code, once known.
    fn server_location(&self) -> Option<String> {
        None
    }
}

pub trait Provider: Send + Sync {
    fn name(&self) -> &str;
    fn host(&self) -> &str;
    fn session(&self, cancel: &CancelToken) -> Result<Box<dyn Session>, NetError>;
}

// ------------------------------------------------------------- Cloudflare

const CLOUDFLARE_BASE: &str = "https://speed.cloudflare.com";
const CHUNK: usize = 64 * 1024;
const MAX_TRANSFER_BYTES: u64 = 1 << 30;

pub struct Cloudflare {
    base: String,
    /// Longest one request body (a chunk) may take before the transfer counts
    /// as stalled. This is a total time per chunk, not an idle time, so it
    /// bounds how slow a connection the benchmark can still measure.
    body_timeout: Duration,
}

impl Cloudflare {
    pub fn new() -> Self {
        Self {
            base: CLOUDFLARE_BASE.into(),
            body_timeout: Duration::from_secs(60),
        }
    }

    /// A provider at another address, for tests against a local server. Plain
    /// HTTP is allowed only there; production code only ever calls `new`.
    #[cfg(test)]
    pub fn with_base(base: &str, body_timeout: Duration) -> Self {
        Self {
            base: base.into(),
            body_timeout,
        }
    }
}

impl Default for Cloudflare {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for Cloudflare {
    fn name(&self) -> &str {
        "Cloudflare speed test"
    }

    fn host(&self) -> &str {
        self.base
            .trim_start_matches("https://")
            .trim_start_matches("http://")
    }

    fn session(&self, cancel: &CancelToken) -> Result<Box<dyn Session>, NetError> {
        let config = ureq::Agent::config_builder()
            .max_redirects(0)
            // Status codes are read from the response, so Retry-After is kept.
            .http_status_as_error(false)
            .https_only(!cfg!(test))
            .timeout_resolve(Some(Duration::from_secs(5)))
            .timeout_connect(Some(Duration::from_secs(5)))
            // A stalled transfer is detected without waiting minutes.
            .timeout_recv_body(Some(self.body_timeout))
            .timeout_send_body(Some(self.body_timeout))
            .timeout_recv_response(Some(self.body_timeout.min(Duration::from_secs(10))))
            .timeout_global(Some(Duration::from_secs(120)))
            .user_agent("TaskManager-Benchmark")
            .build();
        let agent: ureq::Agent = config.into();
        let shutdown = agent.clone();
        cancel.on_cancel(move || shutdown.shutdown());
        Ok(Box::new(CloudflareSession {
            agent,
            base: self.base.clone(),
            colo: None,
        }))
    }
}

struct CloudflareSession {
    agent: ureq::Agent,
    base: String,
    colo: Option<String>,
}

fn map_error(e: ureq::Error) -> NetError {
    match e {
        ureq::Error::Timeout(_) => NetError::Timeout,
        ureq::Error::StatusCode(code) => status_error(code, None),
        ureq::Error::Io(io) if io.kind() == std::io::ErrorKind::TimedOut => NetError::Timeout,
        other => NetError::Unreachable(other.to_string()),
    }
}

/// Errors from reading a body arrive as `io::Error`; a timeout is wrapped
/// inside rather than reported as `TimedOut`.
fn read_error(e: std::io::Error) -> NetError {
    let timed_out = e.kind() == std::io::ErrorKind::TimedOut
        || e.get_ref()
            .and_then(|inner| inner.downcast_ref::<ureq::Error>())
            .is_some_and(|inner| matches!(inner, ureq::Error::Timeout(_)));
    if timed_out {
        NetError::Timeout
    } else {
        NetError::Unreachable(e.to_string())
    }
}

fn status_error(code: u16, retry_after_secs: Option<u64>) -> NetError {
    match code {
        403 | 429 => NetError::Limited {
            code,
            retry_after_secs,
        },
        _ => NetError::BadResponse(format!("HTTP status {code}")),
    }
}

fn require_success(response: &ureq::http::Response<ureq::Body>) -> Result<(), NetError> {
    if response.status().is_success() {
        Ok(())
    } else {
        let retry = response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse().ok());
        Err(status_error(response.status().as_u16(), retry))
    }
}

impl CloudflareSession {
    /// Only the server's own data-centre code is kept. The response also
    /// carries the caller's address and location; those are never read.
    fn note_location(&mut self, response: &ureq::http::Response<ureq::Body>) {
        if self.colo.is_none() {
            self.colo = response
                .headers()
                .get("colo")
                .and_then(|v| v.to_str().ok())
                .filter(|c| c.len() <= 8 && c.bytes().all(|b| b.is_ascii_alphanumeric()))
                .map(str::to_owned);
        }
    }
}

impl Session for CloudflareSession {
    fn ping(&mut self) -> Result<Duration, NetError> {
        let started = Instant::now();
        let mut response = self
            .agent
            .get(format!("{}/__down?bytes=0", self.base))
            .call()
            .map_err(map_error)?;
        require_success(&response)?;
        // A compromised endpoint must not turn a latency probe into an
        // unbounded allocation or transfer.
        let received = std::io::copy(
            &mut response.body_mut().as_reader().take(1025),
            &mut std::io::sink(),
        )
        .map_err(|e| NetError::Unreachable(e.to_string()))?;
        if received > 1024 {
            return Err(NetError::BadResponse(
                "latency response exceeds 1 KiB".into(),
            ));
        }
        let elapsed = started.elapsed();
        self.note_location(&response);
        Ok(elapsed)
    }

    fn download(
        &mut self,
        bytes: u64,
        progress: &mut dyn FnMut(usize) -> bool,
    ) -> Result<(), NetError> {
        if bytes > MAX_TRANSFER_BYTES {
            return Err(NetError::BadResponse(
                "download exceeds the 1 GiB request limit".into(),
            ));
        }
        let mut response = self
            .agent
            .get(format!("{}/__down?bytes={bytes}", self.base))
            .call()
            .map_err(map_error)?;
        require_success(&response)?;
        self.note_location(&response);
        let mut reader = response.body_mut().as_reader();
        let mut buf = vec![0u8; CHUNK];
        let mut total = 0u64;
        loop {
            let limit = (bytes - total + 1).min(CHUNK as u64) as usize;
            let n = reader.read(&mut buf[..limit]).map_err(read_error)?;
            if n == 0 {
                break;
            }
            total += n as u64;
            let keep_reading = progress(n);
            if total > bytes {
                return Err(NetError::BadResponse(
                    "download exceeds the requested size".into(),
                ));
            }
            if !keep_reading {
                // Stopped on purpose; dropping the reader closes the transfer.
                return Ok(());
            }
        }
        if total != bytes {
            return Err(NetError::BadResponse(format!(
                "expected {bytes} bytes, received {total}"
            )));
        }
        Ok(())
    }

    fn upload(
        &mut self,
        bytes: u64,
        progress: &mut dyn FnMut(usize) -> bool,
    ) -> Result<(), NetError> {
        if bytes > MAX_TRANSFER_BYTES {
            return Err(NetError::BadResponse(
                "upload exceeds the 1 GiB request limit".into(),
            ));
        }
        let mut body = GeneratedBody {
            remaining: bytes,
            progress,
            stopped: false,
        };
        let result = self
            .agent
            .post(format!("{}/__up", self.base))
            .header("Content-Length", bytes.to_string())
            .send(ureq::SendBody::from_reader(&mut body));
        match result {
            Ok(response) => require_success(&response),
            // We cut the body short ourselves: not a failure.
            Err(_) if body.stopped => Ok(()),
            Err(e) => Err(map_error(e)),
        }
    }

    fn server_location(&self) -> Option<String> {
        self.colo.clone()
    }
}

/// Deterministic pseudo-random bytes handed to the connection as the upload
/// body. Reports every chunk so the engine counts what was really sent.
struct GeneratedBody<'a> {
    remaining: u64,
    progress: &'a mut dyn FnMut(usize) -> bool,
    stopped: bool,
}

impl Read for GeneratedBody<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.remaining == 0 {
            return Ok(0);
        }
        let n = buf.len().min(CHUNK).min(self.remaining as usize);
        // xorshift fill: cheap, incompressible-looking, and not user data.
        let mut x = 0x9E37_79B9_7F4A_7C15u64 ^ self.remaining;
        for chunk in buf[..n].chunks_mut(8) {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            chunk.copy_from_slice(&x.to_le_bytes()[..chunk.len()]);
        }
        self.remaining -= n as u64;
        if !(self.progress)(n) {
            self.stopped = true;
            return Err(std::io::Error::other("stopped"));
        }
        Ok(n)
    }
}

#[cfg(test)]
pub(crate) mod test_server {
    //! A tiny local HTTP/1.1 server speaking the Cloudflare endpoints, so the
    //! real client code is tested with no internet and no third party.

    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::time::{Duration, Instant};

    #[derive(Clone, Copy, PartialEq, Eq)]
    pub enum Mode {
        Normal,
        /// Promises N bytes, sends half, closes.
        ShortBody,
        /// Sends headers, then nothing.
        Stall,
        StallHeaders,
        StallDownload,
        StallUpload,
        /// Answers with something that is not HTTP.
        Garbage,
        /// Always answers 503.
        Unavailable,
        /// Always answers 429 with a Retry-After of 120 seconds.
        RateLimited,
    }

    pub struct Server {
        pub base: String,
        stop: Arc<AtomicBool>,
        pub uploaded: Arc<AtomicU64>,
    }

    impl Drop for Server {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            // Wake the accept loop.
            let _ = TcpStream::connect(self.base.trim_start_matches("http://"));
        }
    }

    /// `rate` limits response bytes per second (0 = unlimited).
    pub fn start(mode: Mode, rate: u64) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let uploaded = Arc::new(AtomicU64::new(0));
        let (s, u) = (Arc::clone(&stop), Arc::clone(&uploaded));
        std::thread::spawn(move || {
            for conn in listener.incoming() {
                if s.load(Ordering::Relaxed) {
                    break;
                }
                let Ok(conn) = conn else { continue };
                let (s2, u2) = (Arc::clone(&s), Arc::clone(&u));
                std::thread::spawn(move || {
                    let _ = serve(conn, mode, rate, &s2, &u2);
                });
            }
        });
        Server {
            base: format!("http://{addr}"),
            stop,
            uploaded,
        }
    }

    fn serve(
        stream: TcpStream,
        mode: Mode,
        rate: u64,
        stop: &AtomicBool,
        uploaded: &AtomicU64,
    ) -> std::io::Result<()> {
        let mut out = stream.try_clone()?;
        let mut reader = BufReader::new(stream);
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line)? == 0 {
                return Ok(());
            }
            let mut parts = line.split_whitespace();
            let (method, target) = (
                parts.next().unwrap_or(""),
                parts.next().unwrap_or("").to_owned(),
            );
            let mut length = 0u64;
            loop {
                let mut h = String::new();
                reader.read_line(&mut h)?;
                if h.trim().is_empty() {
                    break;
                }
                if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = v.trim().parse().unwrap_or(0);
                }
            }
            if mode == Mode::StallHeaders || (mode == Mode::StallUpload && method == "POST") {
                stall(stop);
                return Ok(());
            }
            match mode {
                Mode::Garbage => return out.write_all(b"this is not http\r\n\r\n"),
                Mode::RateLimited => {
                    out.write_all(b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 120\r\nContent-Length: 0\r\n\r\n")?;
                    continue;
                }
                Mode::Unavailable => {
                    out.write_all(
                        b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\n\r\n",
                    )?;
                    continue;
                }
                _ => {}
            }
            if method == "POST" {
                let mut left = length;
                let mut buf = vec![0u8; 64 * 1024];
                while left > 0 {
                    let n = reader.read(&mut buf[..(left as usize).min(64 * 1024)])?;
                    if n == 0 {
                        return Ok(());
                    }
                    left -= n as u64;
                    uploaded.fetch_add(n as u64, Ordering::Relaxed);
                }
                out.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")?;
                continue;
            }
            let wanted: u64 = target
                .split("bytes=")
                .nth(1)
                .and_then(|n| n.parse().ok())
                .unwrap_or(0);
            write!(
                out,
                "HTTP/1.1 200 OK\r\nContent-Length: {wanted}\r\ncolo: TST\r\n\r\n"
            )?;
            if mode == Mode::Stall || (mode == Mode::StallDownload && wanted > 0) {
                stall(stop);
                return Ok(());
            }
            let send = if mode == Mode::ShortBody {
                wanted / 2
            } else {
                wanted
            };
            let chunk = vec![0xA5u8; 16 * 1024];
            let started = Instant::now();
            let mut sent = 0u64;
            while sent < send {
                let n = (send - sent).min(chunk.len() as u64) as usize;
                out.write_all(&chunk[..n])?;
                sent += n as u64;
                if rate > 0 {
                    let due = Duration::from_secs_f64(sent as f64 / rate as f64);
                    if let Some(wait) = due.checked_sub(started.elapsed()) {
                        std::thread::sleep(wait);
                    }
                }
            }
            if mode == Mode::ShortBody {
                return Ok(());
            }
        }
    }

    fn stall(stop: &AtomicBool) {
        let until = Instant::now() + Duration::from_secs(60);
        while Instant::now() < until && !stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

#[cfg(test)]
mod security_tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    fn server(response: Vec<u8>) -> (String, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let thread = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut request = [0; 4096];
            let _ = stream.read(&mut request);
            let _ = stream.write_all(&response);
        });
        (url, thread)
    }

    #[test]
    fn refuses_redirects_without_contacting_the_destination() {
        let destination = TcpListener::bind("127.0.0.1:0").unwrap();
        destination.set_nonblocking(true).unwrap();
        let response = format!(
            "HTTP/1.1 302 Found\r\nLocation: http://{}/private\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            destination.local_addr().unwrap()
        );
        let (url, thread) = server(response.into_bytes());
        assert!(
            Cloudflare::with_base(&url, Duration::from_secs(3))
                .session(&CancelToken::new())
                .unwrap()
                .ping()
                .is_err()
        );
        thread.join().unwrap();
        assert_eq!(
            destination.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }

    #[test]
    fn refuses_oversized_ping_and_download_bodies() {
        for ping in [true, false] {
            let mut response =
                b"HTTP/1.1 200 OK\r\nContent-Length: 4096\r\nConnection: close\r\n\r\n".to_vec();
            response.extend([b'x'; 4096]);
            let (url, thread) = server(response);
            let mut session = Cloudflare::with_base(&url, Duration::from_secs(3))
                .session(&CancelToken::new())
                .unwrap();
            let mut reported = 0;
            let result = if ping {
                session.ping().map(|_| ())
            } else {
                session.download(1024, &mut |n| {
                    reported += n;
                    true
                })
            };
            assert!(matches!(result, Err(NetError::BadResponse(_))));
            assert_eq!(reported, if ping { 0 } else { 1025 });
            thread.join().unwrap();
        }
    }

    #[test]
    fn rejects_oversized_requests_before_contacting_any_server() {
        let provider = Cloudflare::with_base("http://127.0.0.1:1", Duration::from_secs(1));
        let mut session = provider.session(&CancelToken::new()).unwrap();
        assert!(matches!(
            session.download(MAX_TRANSFER_BYTES + 1, &mut |_| true),
            Err(NetError::BadResponse(_))
        ));
        assert!(matches!(
            session.upload(MAX_TRANSFER_BYTES + 1, &mut |_| true),
            Err(NetError::BadResponse(_))
        ));
    }

    #[test]
    fn rate_limiting_and_refusal_get_their_own_plain_message() {
        let limited = |code, secs| NetError::Limited {
            code,
            retry_after_secs: secs,
        };
        assert_eq!(status_error(429, Some(5)), limited(429, Some(5)));
        assert_eq!(status_error(403, None), limited(403, None));
        assert!(matches!(status_error(503, None), NetError::BadResponse(_)));
        let m = limited(429, Some(3515)).message("example.test");
        assert!(
            m.contains("limiting requests") && m.contains("in about 59 minutes"),
            "{m}"
        );
        assert!(
            limited(429, Some(30))
                .message("x")
                .contains("in about 30 seconds")
        );
        assert!(limited(429, None).message("x").contains("in a few minutes"));
    }
}
