//! Progress reporting with bounded IPC traffic.
//!
//! Workloads may report as often as they like; [`Reporter`] coalesces
//! updates and forwards at most one progress message per
//! [`MIN_PROGRESS_INTERVAL`] (8 per second). Phase changes are always sent
//! immediately so the UI never misses a state transition. Live chart samples
//! are buffered between messages and capped, so a fast backend sampler can
//! never flood the WebView.

use std::time::{Duration, Instant};

use super::models::{
    BenchmarkEvent, BenchmarkKind, BenchmarkPhase, BenchmarkProgress, LiveSample, Measurement,
    MetricUnit,
};

pub const MIN_PROGRESS_INTERVAL: Duration = Duration::from_millis(125);
/// Live samples carried by one message; older ones are thinned out.
pub const MAX_LIVE_PER_MESSAGE: usize = 32;

/// Receives benchmark events. Returns `false` once the receiver is gone
/// (e.g. the window was closed); the run continues without a UI.
pub trait ProgressSink: Send + Sync {
    fn send(&self, event: &BenchmarkEvent) -> bool;
}

/// Discards events (used when nobody is listening).
pub struct NullSink;

impl ProgressSink for NullSink {
    fn send(&self, _event: &BenchmarkEvent) -> bool {
        true
    }
}

pub struct Reporter<'a> {
    sink: &'a dyn ProgressSink,
    connected: bool,
    run_started: Instant,
    last_sent: Option<Instant>,
    state: BenchmarkProgress,
    /// Fraction of the run covered by completed categories, and the share of
    /// the current one, for the whole-run percentage.
    base: f32,
    span: f32,
    pending: bool,
    sent: u64,
}

impl<'a> Reporter<'a> {
    pub fn new(sink: &'a dyn ProgressSink, run_id: &str, run_started: Instant) -> Self {
        Self {
            sink,
            connected: true,
            run_started,
            last_sent: None,
            state: BenchmarkProgress {
                run_id: run_id.to_owned(),
                phase: BenchmarkPhase::Idle,
                kind: None,
                subtest: None,
                percent: 0.0,
                elapsed_ms: 0,
                iteration: None,
                total_iterations: None,
                measurement: None,
                message: String::new(),
                live: Vec::new(),
            },
            base: 0.0,
            span: 1.0,
            pending: false,
            sent: 0,
        }
    }

    pub fn run_id(&self) -> &str {
        &self.state.run_id
    }

    pub fn elapsed_ms(&self) -> u64 {
        u64::try_from(self.run_started.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    /// Number of messages actually delivered (for tests and diagnostics).
    pub fn sent_count(&self) -> u64 {
        self.sent
    }

    /// Declares the slice of the whole run the current category covers.
    pub fn set_category(&mut self, kind: Option<BenchmarkKind>, base: f32, span: f32) {
        self.state.kind = kind;
        self.base = base.clamp(0.0, 1.0);
        self.span = span.clamp(0.0, 1.0 - self.base);
        self.state.subtest = None;
        self.state.iteration = None;
        self.state.total_iterations = None;
        self.state.measurement = None;
        self.state.percent = self.base * 100.0;
    }

    /// Phase transitions are delivered immediately.
    pub fn phase(&mut self, phase: BenchmarkPhase, message: impl Into<String>) {
        self.state.phase = phase;
        self.state.message = message.into();
        if phase == BenchmarkPhase::Completed {
            self.state.percent = 100.0;
        }
        self.flush();
    }

    pub fn subtest(&mut self, label: impl Into<String>) {
        self.state.subtest = Some(label.into());
        self.state.iteration = None;
        self.state.total_iterations = None;
        self.state.measurement = None;
        self.mark();
    }

    /// `fraction` is progress within the current category (0–1).
    pub fn progress(&mut self, fraction: f32, message: impl Into<String>) {
        self.state.percent = (self.base + self.span * fraction.clamp(0.0, 1.0)) * 100.0;
        self.state.message = message.into();
        self.mark();
    }

    pub fn iteration(&mut self, current: u32, total: u32) {
        self.state.iteration = Some(current);
        self.state.total_iterations = Some(total);
        self.mark();
    }

    pub fn measurement(&mut self, label: impl Into<String>, value: f64, unit: MetricUnit) {
        if value.is_finite() {
            self.state.measurement = Some(Measurement {
                label: label.into(),
                value,
                unit,
            });
            self.mark();
        }
    }

    /// Buffers a live chart sample; delivered with the next message.
    pub fn live(&mut self, series: &str, value: f64, unit: MetricUnit) {
        if !value.is_finite() {
            return;
        }
        if self.state.live.len() >= MAX_LIVE_PER_MESSAGE * 2 {
            // Thin out by keeping every other sample; preserves the shape.
            let mut i = 0;
            self.state.live.retain(|_| {
                i += 1;
                i % 2 == 0
            });
        }
        self.state.live.push(LiveSample {
            t_ms: self.elapsed_ms(),
            series: series.to_owned(),
            value,
            unit,
        });
        self.mark();
    }

    /// Sends a raw event (bypassing throttling).
    pub fn event(&mut self, event: &BenchmarkEvent) {
        if self.connected && !self.sink.send(event) {
            self.connected = false;
        }
    }

    fn mark(&mut self) {
        self.pending = true;
        let due = self
            .last_sent
            .is_none_or(|t| t.elapsed() >= MIN_PROGRESS_INTERVAL);
        if due {
            self.flush();
        }
    }

    /// Delivers the current state now.
    pub fn flush(&mut self) {
        self.state.elapsed_ms = self.elapsed_ms();
        if self.state.live.len() > MAX_LIVE_PER_MESSAGE {
            let excess = self.state.live.len() - MAX_LIVE_PER_MESSAGE;
            self.state.live.drain(..excess);
        }
        let event = BenchmarkEvent::Progress(self.state.clone());
        self.state.live.clear();
        self.pending = false;
        self.last_sent = Some(Instant::now());
        self.sent += 1;
        self.event(&event);
    }

    /// Delivers a pending update if one is waiting.
    pub fn flush_pending(&mut self) {
        if self.pending {
            self.flush();
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    pub struct RecordingSink {
        pub events: Mutex<Vec<BenchmarkEvent>>,
    }

    impl ProgressSink for RecordingSink {
        fn send(&self, event: &BenchmarkEvent) -> bool {
            self.events.lock().unwrap().push(event.clone());
            true
        }
    }

    impl RecordingSink {
        pub fn progress(&self) -> Vec<BenchmarkProgress> {
            self.events
                .lock()
                .unwrap()
                .iter()
                .filter_map(|e| match e {
                    BenchmarkEvent::Progress(p) => Some(p.clone()),
                    _ => None,
                })
                .collect()
        }
    }

    #[test]
    fn throttles_rapid_updates_but_keeps_phase_changes() {
        let sink = RecordingSink::default();
        let mut r = Reporter::new(&sink, "run", Instant::now());
        r.phase(BenchmarkPhase::Running, "go");
        for i in 0..10_000 {
            r.progress(i as f32 / 10_000.0, "working");
            r.live("cpu.throughput", f64::from(i), MetricUnit::Mops);
        }
        r.phase(BenchmarkPhase::Completed, "done");
        let progress = sink.progress();
        // Two phase messages plus at most a couple of throttled updates
        // (the loop finishes well within a second).
        assert!(progress.len() <= 12, "sent {} messages", progress.len());
        assert_eq!(progress.first().unwrap().phase, BenchmarkPhase::Running);
        let last = progress.last().unwrap();
        assert_eq!(last.phase, BenchmarkPhase::Completed);
        assert!((last.percent - 100.0).abs() < f32::EPSILON);
        assert!(
            progress
                .iter()
                .all(|p| p.live.len() <= MAX_LIVE_PER_MESSAGE)
        );
    }

    #[test]
    fn percent_maps_category_slice_to_whole_run() {
        let sink = RecordingSink::default();
        let mut r = Reporter::new(&sink, "run", Instant::now());
        r.set_category(Some(BenchmarkKind::Gpu), 0.5, 0.25);
        r.progress(0.5, "half");
        r.flush();
        let p = sink.progress().pop().unwrap();
        assert!((p.percent - 62.5).abs() < 1e-3);
        assert_eq!(p.kind, Some(BenchmarkKind::Gpu));
    }

    #[test]
    fn ignores_non_finite_measurements() {
        let sink = RecordingSink::default();
        let mut r = Reporter::new(&sink, "run", Instant::now());
        r.measurement("x", f64::NAN, MetricUnit::Mops);
        r.live("x", f64::INFINITY, MetricUnit::Mops);
        r.flush();
        let p = sink.progress().pop().unwrap();
        assert!(p.measurement.is_none());
        assert!(p.live.is_empty());
    }
}
