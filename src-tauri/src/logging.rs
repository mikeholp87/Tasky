//! Structured logging.
//!
//! - Rolling daily log files in the app's log directory, at most
//!   [`MAX_LOG_FILES`] kept, so logs never grow without bound.
//! - Default level `warn` (release) / `debug` (development); the "Debug
//!   logging" setting switches at runtime without a restart.
//! - Recent warnings/errors are kept in memory for the diagnostics bundle.
//!
//! Never log command lines, environments, tokens or other secrets.

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::io::{self, Write};
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use serde::Serialize;
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer, Registry, reload};

const MAX_LOG_FILES: usize = 7;
const MAX_LOG_BYTES: u64 = 56 * 1024 * 1024;
const RECENT_LEN: usize = 50;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogRecord {
    pub timestamp_ms: u64,
    pub level: String,
    pub target: String,
    pub message: String,
}

type FilterHandle = reload::Handle<EnvFilter, Registry>;

pub struct Logging {
    filter: Option<FilterHandle>,
    recent: Arc<Mutex<VecDeque<LogRecord>>>,
    /// Flushes buffered file output when dropped (on exit).
    _guard: Option<WorkerGuard>,
}

fn filter_for(debug: bool) -> EnvFilter {
    if let Ok(env) = EnvFilter::try_from_default_env() {
        return env;
    }
    EnvFilter::new(if debug || cfg!(debug_assertions) {
        "warn,taskmanager_lib=debug,tm_core=debug"
    } else {
        "warn"
    })
}

/// Installs the global subscriber. Safe to call once; a second call (e.g. in
/// tests) leaves the existing subscriber in place.
pub fn init(log_dir: Option<&Path>, debug: bool) -> Logging {
    let recent = Arc::new(Mutex::new(VecDeque::with_capacity(RECENT_LEN)));
    let (filter, handle) = reload::Layer::new(filter_for(debug));

    let (file_layer, guard) = match log_dir.and_then(|dir| file_appender(dir).ok()) {
        Some(appender) => {
            let (writer, guard) = tracing_appender::non_blocking(appender);
            let layer = tracing_subscriber::fmt::layer()
                .with_writer(writer)
                .with_ansi(false)
                .with_target(true);
            (Some(layer), Some(guard))
        }
        None => (None, None),
    };
    let stderr_layer =
        cfg!(debug_assertions).then(|| tracing_subscriber::fmt::layer().with_target(false));

    let installed = tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .with(stderr_layer)
        .with(RecentLayer(Arc::clone(&recent)))
        .try_init()
        .is_ok();

    Logging {
        filter: installed.then_some(handle),
        recent,
        _guard: guard,
    }
}

fn file_appender(dir: &Path) -> Result<BoundedLogs, Box<dyn std::error::Error>> {
    tm_core::filesystem::private_dir(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    let inner = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("taskmanager")
        .filename_suffix("log")
        .max_log_files(MAX_LOG_FILES)
        .build(dir)?;
    Ok(BoundedLogs {
        inner,
        used: log_bytes(dir)?,
        scanned_day: utc_day(),
        dir: dir.to_owned(),
        max_bytes: MAX_LOG_BYTES,
    })
}

/// Runs behind tracing-appender's bounded non-blocking queue. At the disk
/// budget, discard further output until daily rotation frees old files.
struct BoundedLogs {
    inner: RollingFileAppender,
    dir: std::path::PathBuf,
    max_bytes: u64,
    /// Bytes in log files, counted from disk only at startup and once per
    /// (UTC) day, when the appender rotates and prunes old files.
    used: u64,
    scanned_day: u64,
}

fn utc_day() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() / 86_400)
}

fn log_bytes(dir: &Path) -> io::Result<u64> {
    Ok(std::fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| n.starts_with("taskmanager.") && n.ends_with(".log"))
        })
        .filter_map(|e| e.path().symlink_metadata().ok())
        .filter(|m| m.is_file())
        .fold(0u64, |sum, m| sum.saturating_add(m.len())))
}

impl Write for BoundedLogs {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.inner.write(&[])?; // Allow rotation even after reaching the cap.
        let day = utc_day();
        if day != self.scanned_day {
            self.used = log_bytes(&self.dir)?;
            self.scanned_day = day;
        }
        if self.used.saturating_add(bytes.len() as u64) > self.max_bytes {
            return Ok(bytes.len());
        }
        let written = self.inner.write(bytes)?;
        self.used = self.used.saturating_add(written as u64);
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl Logging {
    pub fn set_debug(&self, debug: bool) {
        if let Some(handle) = &self.filter
            && let Err(e) = handle.reload(filter_for(debug))
        {
            tracing::warn!("could not change log level: {e}");
        }
    }

    pub fn recent(&self) -> Vec<LogRecord> {
        self.recent
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .cloned()
            .collect()
    }
}

/// Keeps the last warnings and errors in memory for diagnostics.
struct RecentLayer(Arc<Mutex<VecDeque<LogRecord>>>);

impl<S: Subscriber> Layer<S> for RecentLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let level = *event.metadata().level();
        if level > Level::WARN {
            return;
        }
        let mut visitor = MessageVisitor(String::new());
        event.record(&mut visitor);
        let record = LogRecord {
            timestamp_ms: tm_core::monitoring::unix_millis(),
            level: level.to_string(),
            target: event.metadata().target().to_owned(),
            message: visitor.0,
        };
        let mut ring = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if ring.len() == RECENT_LEN {
            ring.pop_front();
        }
        ring.push_back(record);
    }
}

struct MessageVisitor(String);

impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if !self.0.is_empty() {
            self.0.push(' ');
        }
        if field.name() == "message" {
            let _ = write!(self.0, "{value:?}");
        } else {
            let _ = write!(self.0, "{}={value:?}", field.name());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn log_output_stays_within_the_disk_budget() {
        let dir = std::env::temp_dir().join(format!(
            "tm-log-cap-{}-{}",
            std::process::id(),
            tm_core::monitoring::unix_millis()
        ));
        std::fs::create_dir(&dir).unwrap();
        let mut writer = file_appender(&dir).unwrap();
        writer.max_bytes = 4;
        writer.write_all(b"1234").unwrap();
        writer.write_all(b"discard this").unwrap();
        writer.flush().unwrap();
        let used: u64 = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().metadata().unwrap().len())
            .sum();
        assert_eq!(used, 4);
        drop(writer);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn disk_usage_is_rescanned_only_when_the_day_changes() {
        let dir = std::env::temp_dir().join(format!(
            "tm-log-rescan-{}-{}",
            std::process::id(),
            tm_core::monitoring::unix_millis()
        ));
        std::fs::create_dir(&dir).unwrap();
        let mut writer = file_appender(&dir).unwrap();
        writer.max_bytes = 100;
        writer.write_all(b"12345").unwrap();
        assert_eq!(writer.used, 5);
        // Space taken behind the writer's back is not seen within the day...
        std::fs::write(dir.join("taskmanager.old.log"), vec![b'x'; 100]).unwrap();
        writer.write_all(b"678").unwrap();
        assert_eq!(writer.used, 8);
        // ...but is after a day boundary, and further output is discarded.
        writer.scanned_day = 0;
        writer.write_all(b"discard").unwrap();
        writer.flush().unwrap();
        assert!(writer.used >= 108);
        drop(writer);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
