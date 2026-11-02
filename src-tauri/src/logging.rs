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

/// Benchmark lifecycle events (`benchmark_started`, …) are always kept at info
/// level; "debug benchmark logging" raises only the benchmark module to debug.
fn filter_for(debug: bool, benchmark_debug: bool) -> EnvFilter {
    if let Ok(env) = EnvFilter::try_from_default_env() {
        return env;
    }
    let base = if debug || cfg!(debug_assertions) {
        "warn,taskmanager_lib=debug,tm_core=debug"
    } else {
        "warn,tm_core::benchmarks=info"
    };
    if benchmark_debug {
        EnvFilter::new(format!("{base},tm_core::benchmarks=debug"))
    } else {
        EnvFilter::new(base)
    }
}

/// Installs the global subscriber. Safe to call once; a second call (e.g. in
/// tests) leaves the existing subscriber in place.
pub fn init(log_dir: Option<&Path>, debug: bool, benchmark_debug: bool) -> Logging {
    let recent = Arc::new(Mutex::new(VecDeque::with_capacity(RECENT_LEN)));
    let (filter, handle) = reload::Layer::new(filter_for(debug, benchmark_debug));

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

fn file_appender(dir: &Path) -> Result<RollingFileAppender, Box<dyn std::error::Error>> {
    std::fs::create_dir_all(dir)?;
    Ok(RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("taskmanager")
        .filename_suffix("log")
        .max_log_files(MAX_LOG_FILES)
        .build(dir)?)
}

impl Logging {
    pub fn set_debug(&self, debug: bool, benchmark_debug: bool) {
        if let Some(handle) = &self.filter
            && let Err(e) = handle.reload(filter_for(debug, benchmark_debug))
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
