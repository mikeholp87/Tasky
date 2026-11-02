//! The monitoring engine: a single dedicated thread that owns all OS polling.
//!
//! ```text
//! control (mpsc) ──► engine thread ──► Collector::sample()
//!                                   ──► serialize once ──► every TickSink
//!                                   ──► Shared { latest, history }  (short write locks)
//! ```
//!
//! While paused the thread blocks on the control channel and uses no CPU.

use std::collections::HashMap;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, PoisonError, RwLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::errors::{AppError, AppResult};
use crate::models::{HistoryPoint, MetricsTick, MonitoringState, StaticSystemInfo};
use crate::monitoring::collector::{Collector, unix_millis};
use crate::monitoring::history::RingBuffer;

/// One hour of history at the fastest (500 ms) interval.
pub const MAX_HISTORY_POINTS: usize = 7_200;

/// Receives serialized ticks. Returns `false` when the receiver is gone so the
/// engine can drop it.
pub trait TickSink: Send + Sync {
    fn deliver(&self, json: &str) -> bool;
}

enum Control {
    SetState(MonitoringState),
    RefreshNow,
    Shutdown,
}

struct Shared {
    static_info: StaticSystemInfo,
    state: RwLock<MonitoringState>,
    latest: RwLock<Option<Arc<MetricsTick>>>,
    history: RwLock<RingBuffer<HistoryPoint>>,
    sinks: Mutex<HashMap<String, Arc<dyn TickSink>>>,
}

pub struct Engine {
    control: mpsc::Sender<Control>,
    shared: Arc<Shared>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl Engine {
    pub fn start(initial: MonitoringState) -> AppResult<Self> {
        let collector = Collector::new()?;
        let shared = Arc::new(Shared {
            static_info: collector.static_info(),
            state: RwLock::new(initial),
            latest: RwLock::new(None),
            history: RwLock::new(RingBuffer::new(MAX_HISTORY_POINTS)),
            sinks: Mutex::new(HashMap::new()),
        });
        let (tx, rx) = mpsc::channel();
        let thread_shared = Arc::clone(&shared);
        let handle = thread::Builder::new()
            .name("monitoring-engine".into())
            .spawn(move || run(collector, rx, thread_shared, initial))
            .map_err(|e| AppError::SystemQueryFailed(format!("cannot start engine: {e}")))?;
        Ok(Self {
            control: tx,
            shared,
            thread: Mutex::new(Some(handle)),
        })
    }

    pub fn static_info(&self) -> &StaticSystemInfo {
        &self.shared.static_info
    }

    pub fn state(&self) -> MonitoringState {
        *self
            .shared
            .state
            .read()
            .unwrap_or_else(PoisonError::into_inner)
    }

    pub fn set_state(&self, state: MonitoringState) -> AppResult<MonitoringState> {
        *self
            .shared
            .state
            .write()
            .unwrap_or_else(PoisonError::into_inner) = state;
        self.send(Control::SetState(state))?;
        Ok(state)
    }

    pub fn refresh_now(&self) -> AppResult<()> {
        self.send(Control::RefreshNow)
    }

    pub fn latest(&self) -> Option<Arc<MetricsTick>> {
        self.shared
            .latest
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// History points newer than `seconds` ago, oldest first.
    pub fn history(&self, seconds: u32) -> Vec<HistoryPoint> {
        let cutoff = unix_millis().saturating_sub(u64::from(seconds) * 1_000);
        let history = self
            .shared
            .history
            .read()
            .unwrap_or_else(PoisonError::into_inner);
        history.iter().filter(|p| p.t >= cutoff).cloned().collect()
    }

    /// Registers (or replaces) the sink with the given key. Re-subscribing
    /// after a webview reload replaces the stale channel instead of leaking it.
    pub fn subscribe(&self, key: String, sink: Arc<dyn TickSink>) {
        self.shared
            .sinks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(key, sink);
    }

    pub fn shutdown(&self) {
        let _ = self.control.send(Control::Shutdown);
        if let Some(handle) = self
            .thread
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            let _ = handle.join();
        }
    }

    fn send(&self, msg: Control) -> AppResult<()> {
        self.control
            .send(msg)
            .map_err(|_| AppError::EngineUnavailable)
    }
}

fn run(
    mut collector: Collector,
    rx: mpsc::Receiver<Control>,
    shared: Arc<Shared>,
    mut state: MonitoringState,
) {
    let mut next_due = Instant::now() + interval(&state);
    loop {
        let msg = if state.paused {
            rx.recv().map_err(|_| RecvTimeoutError::Disconnected)
        } else {
            rx.recv_timeout(next_due.saturating_duration_since(Instant::now()))
        };
        match msg {
            Ok(Control::SetState(new_state)) => {
                let resumed = state.paused && !new_state.paused;
                state = new_state;
                if resumed {
                    tick(&mut collector, &shared, &state);
                }
                next_due = Instant::now() + interval(&state);
            }
            Ok(Control::RefreshNow) => {
                tick(&mut collector, &shared, &state);
                next_due = Instant::now() + interval(&state);
            }
            Err(RecvTimeoutError::Timeout) => {
                tick(&mut collector, &shared, &state);
                // Schedule from the previous deadline to avoid drift, but
                // never try to "catch up" a backlog after a stall.
                next_due = (next_due + interval(&state)).max(Instant::now());
            }
            Ok(Control::Shutdown) | Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    tracing::info!("monitoring engine stopped");
}

fn interval(state: &MonitoringState) -> Duration {
    Duration::from_millis(u64::from(state.interval_ms))
}

fn tick(collector: &mut Collector, shared: &Shared, state: &MonitoringState) {
    let sample = collector.sample(state.interval_ms);
    let json = match serde_json::to_string(&sample) {
        Ok(json) => json,
        Err(e) => {
            tracing::error!("failed to serialize metrics: {e}");
            return;
        }
    };

    shared
        .history
        .write()
        .unwrap_or_else(PoisonError::into_inner)
        .push(sample.point.clone());
    *shared
        .latest
        .write()
        .unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(sample));

    // Deliver outside the sinks lock; a slow webview must not block subscribe().
    let sinks: Vec<(String, Arc<dyn TickSink>)> = shared
        .sinks
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .map(|(k, s)| (k.clone(), Arc::clone(s)))
        .collect();
    let dead: Vec<String> = sinks
        .into_iter()
        .filter(|(_, sink)| !sink.deliver(&json))
        .map(|(k, _)| k)
        .collect();
    if !dead.is_empty() {
        let mut map = shared.sinks.lock().unwrap_or_else(PoisonError::into_inner);
        for key in dead {
            tracing::debug!("dropping disconnected metrics subscriber {key}");
            map.remove(&key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingSink(AtomicUsize, Mutex<Option<String>>);

    impl TickSink for CountingSink {
        fn deliver(&self, json: &str) -> bool {
            self.0.fetch_add(1, Ordering::SeqCst);
            *self.1.lock().unwrap() = Some(json.to_owned());
            true
        }
    }

    fn wait_for(cond: impl Fn() -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if cond() {
                return true;
            }
            thread::sleep(Duration::from_millis(20));
        }
        false
    }

    /// Integration: the real engine produces well-formed snapshots on this OS.
    #[test]
    fn engine_emits_valid_snapshots() {
        let engine = Engine::start(MonitoringState {
            paused: false,
            interval_ms: 500,
        })
        .expect("engine starts");
        let sink = Arc::new(CountingSink(AtomicUsize::new(0), Mutex::new(None)));
        engine.subscribe("test".into(), sink.clone());

        assert!(wait_for(|| sink.0.load(Ordering::SeqCst) >= 2));
        let tick = engine.latest().expect("has a tick");
        assert!(tick.memory.total_bytes > 0);
        assert!(tick.memory.used_bytes <= tick.memory.total_bytes);
        assert!(!tick.cpu.per_core.is_empty());
        assert!((0.0..=100.0).contains(&tick.cpu.usage_percent));
        assert!(tick.elapsed_ms > 0.0);
        assert!(!engine.history(60).is_empty());

        let json: serde_json::Value =
            serde_json::from_str(sink.1.lock().unwrap().as_deref().unwrap()).unwrap();
        assert!(json["cpu"]["perCore"].is_array());
        assert!(json["memory"]["totalBytes"].as_u64().unwrap() > 0);

        let info = engine.static_info();
        assert!(info.logical_core_count > 0);
        assert_eq!(info.self_pid, std::process::id());
        engine.shutdown();
    }

    #[test]
    fn pause_stops_ticks_and_refresh_now_still_works() {
        let engine = Engine::start(MonitoringState {
            paused: true,
            interval_ms: 500,
        })
        .expect("engine starts");
        let sink = Arc::new(CountingSink(AtomicUsize::new(0), Mutex::new(None)));
        engine.subscribe("test".into(), sink.clone());

        thread::sleep(Duration::from_millis(1_200));
        assert_eq!(
            sink.0.load(Ordering::SeqCst),
            0,
            "paused engine must not poll"
        );

        engine.refresh_now().unwrap();
        assert!(wait_for(|| sink.0.load(Ordering::SeqCst) == 1));
        engine.shutdown();
    }
}
