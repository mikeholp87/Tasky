//! The monitoring engine: a single dedicated thread that owns all OS polling.
//!
//! ```text
//! control (mpsc) ──► engine thread ──► Collector::sample()
//!                                   ──► AlertEvaluator
//!                                   ──► Shared { latest, history, alerts }  (short write locks)
//!                                   ──► every TickSink (JSON serialized once, only if needed)
//! ```
//!
//! While paused the thread blocks on the control channel and uses no CPU.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::errors::{AppError, AppResult};
use crate::models::{
    ActivityEvent, ActivityKind, AlertEvent, AlertRule, HistoryPoint, MetricsTick, MonitoringState,
    ProcessPoint, ProcessRow, ProcessState, StaticSystemInfo,
};
use crate::monitoring::activity::{ActivityConfig, ActivityDetector, ActivityLog, Pending};
use crate::monitoring::alerts::AlertEvaluator;
use crate::monitoring::collector::{Collector, unix_millis};
use crate::monitoring::history::RingBuffer;
use crate::monitoring::process_history::ProcessHistory;
use crate::sync_util::{lock, read, write};

/// One hour of history at the fastest (500 ms) interval.
pub const MAX_HISTORY_POINTS: usize = 7_200;
/// Fired alerts kept for the Alerts page.
const ALERT_LOG_LEN: usize = 100;

/// Receives each tick. Returns `false` when the receiver is gone so the
/// engine can drop it.
pub trait TickSink: Send + Sync {
    /// Whether this sink currently needs the serialized tick. When no sink
    /// does (e.g. the window is hidden), serialization is skipped entirely.
    fn wants_json(&self) -> bool {
        true
    }

    /// `json` is present whenever [`TickSink::wants_json`] returned true. It is
    /// owned so a single interested sink can forward it without copying.
    fn deliver(&self, tick: &MetricsTick, json: Option<String>) -> bool;
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
    process_history: RwLock<ProcessHistory>,
    /// Processes suspended through this app, keyed by (pid, start time).
    /// Windows has no documented per-process "suspended" status, so the
    /// engine reports the state it caused.
    suspended: Mutex<HashSet<(u32, u64)>>,
    alert_rules: RwLock<Arc<[AlertRule]>>,
    /// Linux kernel threads are dropped from ticks unless enabled: they are
    /// often over half of all rows and cannot be acted on.
    include_kernel_threads: AtomicBool,
    /// The UI is showing per-process shared-memory accounting (extra reads).
    memory_detail: AtomicBool,
    alert_log: Mutex<RingBuffer<AlertEvent>>,
    activity_log: Mutex<ActivityLog>,
    activity_config: RwLock<ActivityConfig>,
    sinks: Mutex<HashMap<String, Arc<dyn TickSink>>>,
}

pub struct Engine {
    control: mpsc::SyncSender<Control>,
    refresh_requested: Mutex<Option<Instant>>,
    shared: Arc<Shared>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl Engine {
    pub fn start(initial: MonitoringState) -> AppResult<Self> {
        let (tx, rx) = mpsc::sync_channel(16);
        let (ready_tx, ready_rx) = mpsc::sync_channel::<AppResult<StaticSystemInfo>>(1);
        let (shared_tx, shared_rx) = mpsc::sync_channel::<Arc<Shared>>(1);
        let handle = thread::Builder::new()
            .name("monitoring-engine".into())
            .spawn(move || {
                // sysinfo's GPU handles (Vulkan/NVML) are not `Send`, so the
                // collector is created, used and dropped on this thread only.
                let collector = match Collector::new() {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                if ready_tx.send(Ok(collector.static_info())).is_err() {
                    return;
                }
                let Ok(shared) = shared_rx.recv() else {
                    return;
                };
                let worker = Worker {
                    collector,
                    alerts: AlertEvaluator::default(),
                    detector: ActivityDetector::default(),
                    kernel_threads_shown: false,
                    emitted_activity: 0,
                    shared,
                    last_payload: 0,
                };
                worker.run(rx, initial);
            })
            .map_err(|e| AppError::SystemQueryFailed(format!("cannot start engine: {e}")))?;
        let static_info = ready_rx.recv().map_err(|_| AppError::EngineUnavailable)??;
        let shared = Arc::new(Shared {
            static_info,
            state: RwLock::new(initial),
            latest: RwLock::new(None),
            history: RwLock::new(RingBuffer::new(MAX_HISTORY_POINTS)),
            process_history: RwLock::new(ProcessHistory::default()),
            suspended: Mutex::new(HashSet::new()),
            alert_rules: RwLock::new(Arc::from([])),
            include_kernel_threads: AtomicBool::new(false),
            memory_detail: AtomicBool::new(false),
            alert_log: Mutex::new(RingBuffer::new(ALERT_LOG_LEN)),
            activity_log: Mutex::new(ActivityLog::default()),
            activity_config: RwLock::new(ActivityConfig {
                cpu_spike_percent: 80.0,
                memory_jump_bytes: 512 * 1024 * 1024,
            }),
            sinks: Mutex::new(HashMap::new()),
        });
        shared_tx
            .send(Arc::clone(&shared))
            .map_err(|_| AppError::EngineUnavailable)?;
        Ok(Self {
            control: tx,
            refresh_requested: Mutex::new(None),
            shared,
            thread: Mutex::new(Some(handle)),
        })
    }

    pub fn static_info(&self) -> &StaticSystemInfo {
        &self.shared.static_info
    }

    pub fn state(&self) -> MonitoringState {
        *read(&self.shared.state)
    }

    pub fn set_state(&self, state: MonitoringState) -> AppResult<MonitoringState> {
        crate::models::validate_interval(state.interval_ms)?;
        let mut current = write(&self.shared.state);
        self.send(Control::SetState(state))?;
        *current = state;
        Ok(state)
    }

    pub fn refresh_now(&self) -> AppResult<()> {
        self.refresh_at(Instant::now())
    }

    fn refresh_at(&self, now: Instant) -> AppResult<()> {
        let mut last = lock(&self.refresh_requested);
        if last.is_some_and(|t| now.saturating_duration_since(t) < Duration::from_millis(500)) {
            return Ok(());
        }
        self.send(Control::RefreshNow)?;
        *last = Some(now);
        Ok(())
    }

    pub fn latest(&self) -> Option<Arc<MetricsTick>> {
        read(&self.shared.latest).clone()
    }

    /// History points newer than `seconds` ago, oldest first.
    pub fn history(&self, seconds: u32) -> Vec<HistoryPoint> {
        let cutoff = unix_millis().saturating_sub(u64::from(seconds) * 1_000);
        read(&self.shared.history)
            .iter()
            .filter(|p| p.t >= cutoff)
            .cloned()
            .collect()
    }

    /// Resizes the system history buffer (e.g. from the "history length" setting).
    pub fn set_history_capacity(&self, points: usize) {
        write(&self.shared.history).set_capacity(points.clamp(60, MAX_HISTORY_POINTS));
    }

    /// Turns per-process proportional memory (PSS) sampling on or off.
    pub fn set_memory_detail(&self, enabled: bool) {
        self.shared.memory_detail.store(enabled, Ordering::Relaxed);
    }

    /// Whether kernel threads appear in process lists (takes effect next tick).
    pub fn set_include_kernel_threads(&self, include: bool) {
        self.shared
            .include_kernel_threads
            .store(include, Ordering::Relaxed);
    }

    /// The latest row for `pid`, if it is still running.
    pub fn process(&self, pid: u32) -> Option<ProcessRow> {
        self.latest()?
            .processes
            .iter()
            .find(|p| p.pid == pid)
            .cloned()
    }

    /// Recent samples for the process instance `(pid, start_time)`.
    pub fn process_history(&self, pid: u32, start_time: u64) -> Vec<ProcessPoint> {
        read(&self.shared.process_history).get(pid, start_time)
    }

    /// Records that this app suspended or resumed a process.
    pub fn set_suspended(&self, pid: u32, start_time: u64, suspended: bool) {
        let mut set = lock(&self.shared.suspended);
        if suspended {
            set.insert((pid, start_time));
        } else {
            set.remove(&(pid, start_time));
        }
    }

    /// Replaces the alert rules; takes effect on the next tick.
    pub fn set_alert_rules(&self, rules: Vec<AlertRule>) {
        *write(&self.shared.alert_rules) = rules.into();
    }

    /// Recently fired alerts, newest last.
    pub fn recent_alerts(&self) -> Vec<AlertEvent> {
        lock(&self.shared.alert_log).iter().cloned().collect()
    }

    /// Sets the thresholds for CPU spike and memory jump events.
    pub fn set_activity_config(&self, cpu_spike_percent: u32, memory_jump_mb: u32) {
        *write(&self.shared.activity_config) = ActivityConfig {
            cpu_spike_percent: cpu_spike_percent as f32,
            memory_jump_bytes: u64::from(memory_jump_mb) * 1024 * 1024,
        };
    }

    /// Adds an event from outside the engine (a process action, a benchmark).
    /// It reaches the UI with the next tick.
    pub fn record_activity(&self, event: Pending) {
        lock(&self.shared.activity_log).record(unix_millis(), event);
    }

    /// Timeline events with a sequence number above `after`, oldest first.
    pub fn activity_since(&self, after: u64) -> Vec<ActivityEvent> {
        lock(&self.shared.activity_log).since(after)
    }

    /// Registers (or replaces) the sink with the given key. Re-subscribing
    /// after a webview reload replaces the stale channel instead of leaking it.
    pub fn subscribe(&self, key: String, sink: Arc<dyn TickSink>) {
        lock(&self.shared.sinks).insert(key, sink);
    }

    pub fn shutdown(&self) {
        let _ = self.control.send(Control::Shutdown);
        if let Some(handle) = lock(&self.thread).take() {
            let _ = handle.join();
        }
    }

    fn send(&self, msg: Control) -> AppResult<()> {
        self.control.try_send(msg).map_err(|e| match e {
            mpsc::TrySendError::Full(_) => AppError::OperationBusy,
            mpsc::TrySendError::Disconnected(_) => AppError::EngineUnavailable,
        })
    }
}

/// State owned by the engine thread; never shared, so never locked.
struct Worker {
    collector: Collector,
    alerts: AlertEvaluator,
    detector: ActivityDetector,
    /// Whether the previous tick included kernel threads; a change makes
    /// rows appear or vanish without any process starting or exiting.
    kernel_threads_shown: bool,
    /// Highest activity sequence already delivered in a tick.
    emitted_activity: u64,
    shared: Arc<Shared>,
    last_payload: u32,
}

impl Worker {
    fn run(mut self, rx: mpsc::Receiver<Control>, mut state: MonitoringState) {
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
                        self.tick(&state);
                    }
                    next_due = Instant::now() + interval(&state);
                }
                Ok(Control::RefreshNow) => {
                    self.tick(&state);
                    next_due = Instant::now() + interval(&state);
                }
                Err(RecvTimeoutError::Timeout) => {
                    self.tick(&state);
                    // Schedule from the previous deadline to avoid drift, but
                    // never try to "catch up" a backlog after a stall.
                    next_due = (next_due + interval(&state)).max(Instant::now());
                }
                Ok(Control::Shutdown) | Err(RecvTimeoutError::Disconnected) => break,
            }
        }
        tracing::info!("monitoring engine stopped");
    }

    /// Collects, evaluates, publishes and delivers one sample.
    fn tick(&mut self, state: &MonitoringState) {
        let shared = &*self.shared;
        let mut sample = self.collector.sample(
            state.interval_ms,
            self.last_payload,
            shared.memory_detail.load(Ordering::Relaxed),
        );
        let kernel_threads = shared.include_kernel_threads.load(Ordering::Relaxed);
        if !kernel_threads {
            sample.processes.retain(|p| !p.kernel_thread);
            sample.engine.process_count = u32::try_from(sample.processes.len()).unwrap_or(u32::MAX);
        }
        apply_suspended(shared, &mut sample.processes);

        let rules = Arc::clone(&read(&shared.alert_rules));
        sample.alerts = self.alerts.evaluate(&rules, &sample);
        if !sample.alerts.is_empty() {
            let mut log = lock(&shared.alert_log);
            for event in &sample.alerts {
                tracing::info!(rule = %event.rule_id, "alert fired");
                log.push(event.clone());
            }
        }

        // Timeline: diff against the previous snapshot, add alerts, then
        // deliver everything recorded since the last tick (including events
        // other threads recorded in between).
        if kernel_threads != self.kernel_threads_shown {
            self.kernel_threads_shown = kernel_threads;
            self.detector.reset();
        }
        let cfg = *read(&shared.activity_config);
        let found = self
            .detector
            .observe(sample.timestamp_ms, &sample.processes, &cfg);
        {
            let mut log = lock(&shared.activity_log);
            for event in found {
                log.record(sample.timestamp_ms, event);
            }
            for alert in &sample.alerts {
                let mut e = Pending::new(ActivityKind::AlertFired).detail(alert.message.clone());
                e.pid = alert.pid;
                e.name = Some(alert.rule_name.clone());
                log.record(alert.timestamp_ms, e);
            }
            sample.activity = log.since(self.emitted_activity);
            self.emitted_activity = log.last_seq();
        }

        write(&shared.history).push(sample.point.clone());
        write(&shared.process_history).record(sample.timestamp_ms, &sample.processes);
        let sample = Arc::new(sample);
        *write(&shared.latest) = Some(Arc::clone(&sample));

        // Deliver outside the sinks lock; a slow webview must not block subscribe().
        let sinks: Vec<(String, Arc<dyn TickSink>)> = lock(&shared.sinks)
            .iter()
            .map(|(k, s)| (k.clone(), Arc::clone(s)))
            .collect();
        let wants: Vec<bool> = sinks.iter().map(|(_, s)| s.wants_json()).collect();
        let mut json_sinks_left = wants.iter().filter(|w| **w).count();
        let mut json = if json_sinks_left > 0 {
            match serde_json::to_string(&*sample) {
                Ok(json) => {
                    self.last_payload = u32::try_from(json.len()).unwrap_or(u32::MAX);
                    Some(json)
                }
                Err(e) => {
                    tracing::error!("failed to serialize metrics: {e}");
                    None
                }
            }
        } else {
            None
        };
        let mut dead: Vec<String> = Vec::new();
        for ((key, sink), wants) in sinks.into_iter().zip(wants) {
            let payload = if wants {
                json_sinks_left -= 1;
                // The last interested sink takes the string itself.
                if json_sinks_left == 0 {
                    json.take()
                } else {
                    json.clone()
                }
            } else {
                None
            };
            if !sink.deliver(&sample, payload) {
                dead.push(key);
            }
        }
        if !dead.is_empty() {
            let mut map = lock(&shared.sinks);
            for key in dead {
                tracing::debug!("dropping disconnected metrics subscriber {key}");
                map.remove(&key);
            }
        }
    }
}

/// Marks processes this app suspended, and forgets ones that have exited.
fn apply_suspended(shared: &Shared, rows: &mut [ProcessRow]) {
    let mut set = lock(&shared.suspended);
    if set.is_empty() {
        return;
    }
    let mut alive = HashSet::with_capacity(set.len());
    for row in rows.iter_mut() {
        let key = (row.pid, row.start_time);
        if set.contains(&key) {
            row.status = ProcessState::Stopped;
            alive.insert(key);
        }
    }
    *set = alive;
}

fn interval(state: &MonitoringState) -> Duration {
    Duration::from_millis(u64::from(state.interval_ms))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::AlertMetric;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    struct CountingSink {
        count: AtomicUsize,
        last_json: Mutex<Option<String>>,
        alerts: Mutex<Vec<AlertEvent>>,
        wants_json: AtomicBool,
    }

    impl CountingSink {
        fn new(wants_json: bool) -> Arc<Self> {
            Arc::new(Self {
                count: AtomicUsize::new(0),
                last_json: Mutex::new(None),
                alerts: Mutex::new(Vec::new()),
                wants_json: AtomicBool::new(wants_json),
            })
        }
    }

    impl TickSink for CountingSink {
        fn wants_json(&self) -> bool {
            self.wants_json.load(Ordering::SeqCst)
        }
        fn deliver(&self, tick: &MetricsTick, json: Option<String>) -> bool {
            self.count.fetch_add(1, Ordering::SeqCst);
            *self.last_json.lock().unwrap() = json;
            self.alerts
                .lock()
                .unwrap()
                .extend(tick.alerts.iter().cloned());
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

    fn engine(paused: bool) -> Engine {
        Engine::start(MonitoringState {
            paused,
            interval_ms: 500,
        })
        .expect("engine starts")
    }

    /// Integration: the real engine produces well-formed snapshots on this OS.
    #[test]
    fn engine_emits_valid_snapshots() {
        let engine = engine(false);
        let sink = CountingSink::new(true);
        engine.subscribe("test".into(), sink.clone());

        assert!(wait_for(|| sink.count.load(Ordering::SeqCst) >= 2));
        let tick = engine.latest().expect("has a tick");
        assert!(tick.memory.total_bytes > 0);
        assert!(tick.memory.used_bytes <= tick.memory.total_bytes);
        assert!(!tick.cpu.per_core.is_empty());
        assert!((0.0..=100.0).contains(&tick.cpu.usage_percent));
        assert!(tick.elapsed_ms > 0.0);
        assert!(!engine.history(60).is_empty());

        let json: serde_json::Value =
            serde_json::from_str(sink.last_json.lock().unwrap().as_deref().unwrap()).unwrap();
        assert!(json["cpu"]["perCore"].is_array());
        assert!(json["memory"]["totalBytes"].as_u64().unwrap() > 0);

        let info = engine.static_info();
        assert!(info.logical_core_count > 0);
        assert_eq!(info.self_pid, std::process::id());
        engine.shutdown();
    }

    #[test]
    fn pause_stops_ticks_and_refresh_now_still_works() {
        let engine = engine(true);
        let sink = CountingSink::new(true);
        engine.subscribe("test".into(), sink.clone());

        thread::sleep(Duration::from_millis(1_200));
        assert_eq!(
            sink.count.load(Ordering::SeqCst),
            0,
            "paused engine must not poll"
        );

        engine.refresh_now().unwrap();
        assert!(wait_for(|| sink.count.load(Ordering::SeqCst) == 1));
        engine.shutdown();
    }

    #[test]
    fn repeated_refresh_requests_are_coalesced_at_the_backend() {
        let engine = engine(true);
        let sink = CountingSink::new(false);
        engine.subscribe("refresh-limit".into(), sink.clone());
        let now = Instant::now();
        for _ in 0..10_000 {
            engine.refresh_at(now).unwrap();
        }
        assert!(wait_for(|| sink.count.load(Ordering::SeqCst) == 1));
        engine.shutdown();
        assert_eq!(sink.count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn skips_serialization_when_no_sink_wants_json() {
        let engine = engine(true);
        let sink = CountingSink::new(false);
        engine.subscribe("hidden".into(), sink.clone());
        engine.refresh_now().unwrap();
        assert!(wait_for(|| sink.count.load(Ordering::SeqCst) == 1));
        assert!(sink.last_json.lock().unwrap().is_none());
        engine.shutdown();
    }

    #[test]
    fn every_sink_that_wants_json_gets_it() {
        let engine = engine(true);
        let (a, b, hidden) = (
            CountingSink::new(true),
            CountingSink::new(true),
            CountingSink::new(false),
        );
        engine.subscribe("a".into(), a.clone());
        engine.subscribe("b".into(), b.clone());
        engine.subscribe("hidden".into(), hidden.clone());
        engine.refresh_now().unwrap();
        assert!(wait_for(|| {
            [&a, &b, &hidden]
                .iter()
                .all(|s| s.count.load(Ordering::SeqCst) == 1)
        }));
        let json = a.last_json.lock().unwrap().clone();
        assert!(json.is_some());
        assert_eq!(*b.last_json.lock().unwrap(), json);
        assert!(hidden.last_json.lock().unwrap().is_none());
        engine.shutdown();
    }

    #[test]
    fn evaluates_alert_rules_and_logs_events() {
        let engine = engine(true);
        let sink = CountingSink::new(false);
        engine.subscribe("alerts".into(), sink.clone());
        // Memory use is always above 0%, so this fires on the first tick.
        engine.set_alert_rules(vec![AlertRule {
            id: "always".into(),
            name: "Always".into(),
            enabled: true,
            metric: AlertMetric::SystemMemory,
            threshold: 0.0,
            duration_secs: 0,
            process_name: None,
        }]);
        engine.refresh_now().unwrap();
        assert!(wait_for(|| !sink.alerts.lock().unwrap().is_empty()));
        assert_eq!(engine.recent_alerts()[0].rule_id, "always");
        engine.shutdown();
    }
}
