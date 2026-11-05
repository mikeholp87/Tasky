//! Activity timeline: process starts and exits, CPU spikes and memory jumps,
//! found by comparing successive process snapshots, plus events other parts
//! of the app record (alerts, process actions, benchmarks).
//!
//! Processes are identified by `(pid, start_time)` so a reused PID is an exit
//! followed by a start, never a continuation.

use std::collections::{HashMap, VecDeque};

use crate::models::{ActivityEvent, ActivityKind, ProcessRow};
use crate::monitoring::history::RingBuffer;

/// How long CPU must stay above the threshold before it is a spike; shorter
/// bursts are normal behaviour, not something to report.
const SPIKE_MIN_MS: u64 = 3_000;
/// A spike ends when CPU drops below this share of the threshold, so a
/// process hovering around the line does not flap.
const SPIKE_END_RATIO: f32 = 0.75;
/// Memory growth is measured against the lowest value in this window.
const MEMORY_WINDOW_MS: u64 = 30_000;
/// A mass start or exit (e.g. closing a browser) cannot flood the log.
const MAX_EVENTS_PER_TICK: usize = 50;
/// Events kept for the Activity page.
pub const LOG_LEN: usize = 1_000;

// Slow-growth hints. One sample per minute for up to an hour, so "sustained"
// means ten or more minutes of mostly non-decreasing readings. Thresholds are
// deliberately conservative: a hint is a prompt to look, not a verdict.
const TREND_STEP_MS: u64 = 60_000;
const TREND_SAMPLES: usize = 60;
const TREND_MIN_SAMPLES: usize = 11;
const MEMORY_GROWTH_MIN_BYTES: u64 = 256 * 1024 * 1024;
const MEMORY_GROWTH_RATIO: f64 = 1.25;
const THREAD_GROWTH_MIN: u64 = 32;
const THREAD_GROWTH_RATIO: f64 = 1.5;
/// Share of minute-to-minute steps that must not fall (1% jitter allowed).
const RISING_SHARE: f64 = 0.8;

#[derive(Debug, Clone, Copy)]
pub struct ActivityConfig {
    /// Percent of the whole machine.
    pub cpu_spike_percent: f32,
    pub memory_jump_bytes: u64,
}

/// An event before it is given a sequence number and timestamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pending {
    pub kind: ActivityKind,
    pub pid: Option<u32>,
    pub name: Option<String>,
    pub detail: Option<String>,
}

impl Pending {
    pub fn new(kind: ActivityKind) -> Self {
        Self {
            kind,
            pid: None,
            name: None,
            detail: None,
        }
    }

    pub fn process(kind: ActivityKind, pid: u32, name: &str) -> Self {
        Self {
            pid: Some(pid),
            name: Some(name.to_owned()),
            ..Self::new(kind)
        }
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

struct TrendSample {
    at: u64,
    memory: u64,
    threads: Option<u32>,
}

/// Coarse per-process history for the growth hints; at most 60 small samples.
#[derive(Default)]
struct Trend {
    samples: VecDeque<TrendSample>,
    memory_reported: bool,
    threads_reported: bool,
}

/// Whether `values` rose by at least `min_growth` and `min_ratio` overall
/// without falling back much along the way. Needs ten minutes of samples.
fn sustained_rise(values: &[u64], min_growth: u64, min_ratio: f64) -> bool {
    let (Some(&first), Some(&last)) = (values.first(), values.last()) else {
        return false;
    };
    if values.len() < TREND_MIN_SAMPLES
        || last < first.saturating_add(min_growth)
        || (last as f64) < (first as f64) * min_ratio
    {
        return false;
    }
    let steps = values.len() - 1;
    let holding = values
        .windows(2)
        .filter(|w| w[1].saturating_add(w[0] / 100) >= w[0])
        .count();
    holding as f64 >= steps as f64 * RISING_SHARE
}

fn human_bytes(bytes: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    let mb = bytes as f64 / MB;
    if mb >= 1024.0 {
        format!("{:.1} GB", mb / 1024.0)
    } else {
        format!("{mb:.0} MB")
    }
}

impl Trend {
    /// Adds a sample when a minute has passed and reports hints that newly
    /// hold. Each hint is reported once per process instance.
    fn observe(&mut self, now_ms: u64, row: &ProcessRow) -> Vec<Pending> {
        let due = self
            .samples
            .back()
            .is_none_or(|last| now_ms.saturating_sub(last.at) >= TREND_STEP_MS);
        if !due {
            return Vec::new();
        }
        if self.samples.len() == TREND_SAMPLES {
            self.samples.pop_front();
        }
        self.samples.push_back(TrendSample {
            at: now_ms,
            memory: row.memory_bytes,
            threads: row.threads,
        });

        let mut out = Vec::new();
        let (Some(first), Some(last)) = (self.samples.front(), self.samples.back()) else {
            return out;
        };
        let minutes = last.at.saturating_sub(first.at) / 60_000;
        if !self.memory_reported {
            let values: Vec<u64> = self.samples.iter().map(|s| s.memory).collect();
            if sustained_rise(&values, MEMORY_GROWTH_MIN_BYTES, MEMORY_GROWTH_RATIO) {
                self.memory_reported = true;
                out.push(
                    Pending::process(ActivityKind::MemoryGrowth, row.pid, &row.name).detail(
                        format!(
                            "Sustained memory growth: {} to {} over {minutes} minutes. \
                             A growing cache can be normal.",
                            human_bytes(first.memory),
                            human_bytes(last.memory)
                        ),
                    ),
                );
            }
        }
        if !self.threads_reported {
            let values: Option<Vec<u64>> = self
                .samples
                .iter()
                .map(|s| s.threads.map(u64::from))
                .collect();
            if let Some(values) = values
                && sustained_rise(&values, THREAD_GROWTH_MIN, THREAD_GROWTH_RATIO)
            {
                self.threads_reported = true;
                out.push(
                    Pending::process(ActivityKind::ThreadGrowth, row.pid, &row.name).detail(
                        format!(
                            "Possible resource leak: thread count rose from {} to {} over \
                             {minutes} minutes.",
                            values.first().copied().unwrap_or(0),
                            values.last().copied().unwrap_or(0)
                        ),
                    ),
                );
            }
        }
        out
    }
}

struct Tracked {
    name: String,
    trend: Trend,
    hot_since: Option<u64>,
    spiking: bool,
    mem_ref: u64,
    mem_ref_at: u64,
}

#[derive(Default)]
pub struct ActivityDetector {
    known: HashMap<(u32, u64), Tracked>,
    primed: bool,
}

impl ActivityDetector {
    /// Forgets everything; the next snapshot is adopted silently. Used when
    /// the visible process set changes for a reason other than processes
    /// starting or exiting (e.g. toggling kernel threads).
    pub fn reset(&mut self) {
        self.known.clear();
        self.primed = false;
    }

    pub fn observe(
        &mut self,
        now_ms: u64,
        rows: &[ProcessRow],
        cfg: &ActivityConfig,
    ) -> Vec<Pending> {
        let announce = self.primed;
        self.primed = true;
        let mut out: Vec<Pending> = Vec::new();
        // Starts and exits over the cap are counted, not lost: a summary line
        // follows so the timeline shows that something happened.
        let (mut more_started, mut more_exited) = (0usize, 0usize);
        let mut push = |p: Pending| {
            if out.len() < MAX_EVENTS_PER_TICK {
                out.push(p);
            } else {
                match p.kind {
                    ActivityKind::ProcessStarted => more_started += 1,
                    ActivityKind::ProcessExited => more_exited += 1,
                    _ => {}
                }
            }
        };

        let mut seen = std::collections::HashSet::with_capacity(rows.len());
        for row in rows {
            let key = (row.pid, row.start_time);
            seen.insert(key);
            let Some(t) = self.known.get_mut(&key) else {
                if announce {
                    push(Pending::process(
                        ActivityKind::ProcessStarted,
                        row.pid,
                        &row.name,
                    ));
                }
                let mut trend = Trend::default();
                trend.observe(now_ms, row);
                self.known.insert(
                    key,
                    Tracked {
                        name: row.name.clone(),
                        trend,
                        hot_since: None,
                        spiking: false,
                        mem_ref: row.memory_bytes,
                        mem_ref_at: now_ms,
                    },
                );
                continue;
            };

            // CPU spike with a minimum duration and hysteresis. Restricted
            // processes report no CPU, so they never spike.
            if !row.restricted {
                if row.cpu_percent >= cfg.cpu_spike_percent {
                    let since = *t.hot_since.get_or_insert(now_ms);
                    if !t.spiking && now_ms.saturating_sub(since) >= SPIKE_MIN_MS {
                        t.spiking = true;
                        push(
                            Pending::process(ActivityKind::CpuSpike, row.pid, &row.name)
                                .detail(format!("{:.0}% of CPU", row.cpu_percent)),
                        );
                    }
                } else if row.cpu_percent < cfg.cpu_spike_percent * SPIKE_END_RATIO {
                    t.hot_since = None;
                    if t.spiking {
                        t.spiking = false;
                        push(
                            Pending::process(ActivityKind::CpuNormal, row.pid, &row.name)
                                .detail(format!("{:.0}% of CPU", row.cpu_percent)),
                        );
                    }
                }
            }

            // Memory growth against the window's low-water mark.
            if now_ms.saturating_sub(t.mem_ref_at) >= MEMORY_WINDOW_MS
                || row.memory_bytes < t.mem_ref
            {
                t.mem_ref = row.memory_bytes;
                t.mem_ref_at = now_ms;
            } else if row.memory_bytes - t.mem_ref >= cfg.memory_jump_bytes {
                push(
                    Pending::process(ActivityKind::MemoryJump, row.pid, &row.name).detail(format!(
                        "Memory grew by {} MB to {} MB",
                        (row.memory_bytes - t.mem_ref) / (1024 * 1024),
                        row.memory_bytes / (1024 * 1024)
                    )),
                );
                // Re-arm so continued growth is reported in steps, not every tick.
                t.mem_ref = row.memory_bytes;
                t.mem_ref_at = now_ms;
            }

            for hint in t.trend.observe(now_ms, row) {
                push(hint);
            }
        }

        let gone: Vec<(u32, u64)> = self
            .known
            .keys()
            .filter(|k| !seen.contains(k))
            .copied()
            .collect();
        for key in gone {
            if let Some(t) = self.known.remove(&key)
                && announce
            {
                push(Pending::process(
                    ActivityKind::ProcessExited,
                    key.0,
                    &t.name,
                ));
            }
        }
        for (kind, n) in [
            (ActivityKind::ProcessStarted, more_started),
            (ActivityKind::ProcessExited, more_exited),
        ] {
            if n > 0 {
                let mut e = Pending::new(kind);
                e.name = Some(format!("{n} more processes"));
                out.push(e);
            }
        }
        out
    }
}

/// Bounded log with monotonically increasing sequence numbers.
pub struct ActivityLog {
    events: RingBuffer<ActivityEvent>,
    next_seq: u64,
}

impl Default for ActivityLog {
    fn default() -> Self {
        Self {
            events: RingBuffer::new(LOG_LEN),
            next_seq: 1,
        }
    }
}

impl ActivityLog {
    pub fn record(&mut self, timestamp_ms: u64, p: Pending) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.events.push(ActivityEvent {
            seq,
            timestamp_ms,
            kind: p.kind,
            pid: p.pid,
            name: p.name,
            detail: p.detail,
        });
        seq
    }

    /// Events with a sequence number above `after`, oldest first.
    pub fn since(&self, after: u64) -> Vec<ActivityEvent> {
        self.events
            .iter()
            .filter(|e| e.seq > after)
            .cloned()
            .collect()
    }

    /// Sequence number of the newest event, or 0.
    pub fn last_seq(&self) -> u64 {
        self.next_seq - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ProcessState;

    const CFG: ActivityConfig = ActivityConfig {
        cpu_spike_percent: 80.0,
        memory_jump_bytes: 500 * 1024 * 1024,
    };
    const MB: u64 = 1024 * 1024;

    fn row(pid: u32, start: u64, cpu: f32, mem_mb: u64) -> ProcessRow {
        ProcessRow {
            pid,
            parent_pid: None,
            name: format!("p{pid}"),
            status: ProcessState::Running,
            cpu_percent: cpu,
            memory_bytes: mem_mb * MB,
            virtual_memory_bytes: 0,
            disk_read_per_sec: 0.0,
            disk_write_per_sec: 0.0,
            user: None,
            start_time: start,
            threads: None,
            priority: None,
            exe: None,
            cmd: None,
            session_id: None,
            kernel_thread: false,
            pss_bytes: None,
            restricted: false,
        }
    }

    fn kinds(events: &[Pending]) -> Vec<ActivityKind> {
        events.iter().map(|e| e.kind).collect()
    }

    #[test]
    fn first_snapshot_is_adopted_silently() {
        let mut d = ActivityDetector::default();
        assert!(
            d.observe(0, &[row(1, 10, 0.0, 10), row(2, 10, 0.0, 10)], &CFG)
                .is_empty()
        );
        assert!(
            d.observe(1_000, &[row(1, 10, 0.0, 10), row(2, 10, 0.0, 10)], &CFG)
                .is_empty()
        );
    }

    #[test]
    fn reports_starts_exits_and_pid_reuse() {
        let mut d = ActivityDetector::default();
        d.observe(0, &[row(1, 10, 0.0, 10)], &CFG);
        let ev = d.observe(1_000, &[row(1, 10, 0.0, 10), row(2, 50, 0.0, 10)], &CFG);
        assert_eq!(kinds(&ev), [ActivityKind::ProcessStarted]);
        assert_eq!(ev[0].pid, Some(2));

        // Pid 2 exits and the number is reused by a new instance.
        let ev = d.observe(2_000, &[row(1, 10, 0.0, 10), row(2, 99, 0.0, 10)], &CFG);
        assert_eq!(
            kinds(&ev),
            [ActivityKind::ProcessStarted, ActivityKind::ProcessExited]
        );
    }

    #[test]
    fn cpu_spike_needs_duration_and_ends_with_hysteresis() {
        let mut d = ActivityDetector::default();
        d.observe(0, &[row(1, 1, 5.0, 10)], &CFG);
        // A 2 s burst is not a spike.
        assert!(d.observe(1_000, &[row(1, 1, 95.0, 10)], &CFG).is_empty());
        assert!(d.observe(3_000, &[row(1, 1, 95.0, 10)], &CFG).is_empty());
        let ev = d.observe(4_000, &[row(1, 1, 90.0, 10)], &CFG);
        assert_eq!(kinds(&ev), [ActivityKind::CpuSpike]);
        // Reported once per episode.
        assert!(d.observe(5_000, &[row(1, 1, 90.0, 10)], &CFG).is_empty());
        // Dipping just under the threshold is not "normal" yet (80 * 0.75 = 60).
        assert!(d.observe(6_000, &[row(1, 1, 70.0, 10)], &CFG).is_empty());
        let ev = d.observe(7_000, &[row(1, 1, 20.0, 10)], &CFG);
        assert_eq!(kinds(&ev), [ActivityKind::CpuNormal]);
        // A brief dip resets the clock for a later episode.
        assert!(d.observe(8_000, &[row(1, 1, 99.0, 10)], &CFG).is_empty());
    }

    #[test]
    fn memory_jump_is_measured_against_the_window_low_and_rearms() {
        let mut d = ActivityDetector::default();
        d.observe(0, &[row(1, 1, 0.0, 100)], &CFG);
        assert!(d.observe(1_000, &[row(1, 1, 0.0, 400)], &CFG).is_empty());
        let ev = d.observe(2_000, &[row(1, 1, 0.0, 700)], &CFG);
        assert_eq!(kinds(&ev), [ActivityKind::MemoryJump]);
        assert!(ev[0].detail.as_deref().unwrap().contains("600 MB"));
        // Re-armed at 700: no repeat until another 500 MB.
        assert!(d.observe(3_000, &[row(1, 1, 0.0, 800)], &CFG).is_empty());
        // Falling resets the low-water mark.
        assert!(d.observe(4_000, &[row(1, 1, 0.0, 50)], &CFG).is_empty());
        assert_eq!(
            kinds(&d.observe(5_000, &[row(1, 1, 0.0, 600)], &CFG)),
            [ActivityKind::MemoryJump]
        );
    }

    fn grow(
        d: &mut ActivityDetector,
        minutes: u64,
        mem_at: impl Fn(u64) -> u64,
        threads_at: impl Fn(u64) -> u32,
    ) -> Vec<Pending> {
        let mut all = Vec::new();
        for m in 0..=minutes {
            let mut r = row(1, 1, 0.0, 0);
            r.memory_bytes = mem_at(m);
            r.threads = Some(threads_at(m));
            all.extend(d.observe(m * 60_000, &[r], &CFG));
        }
        all
    }

    #[test]
    fn reports_sustained_memory_growth_once() {
        let mut d = ActivityDetector::default();
        // 400 MB rising 100 MB a minute for 12 minutes.
        let ev = grow(&mut d, 12, |m| (400 + 100 * m) * MB, |_| 10);
        let growth: Vec<_> = ev
            .iter()
            .filter(|e| e.kind == ActivityKind::MemoryGrowth)
            .collect();
        assert_eq!(growth.len(), 1, "reported once");
        let detail = growth[0].detail.as_deref().unwrap();
        assert!(
            detail.contains("400 MB") && detail.contains("minutes"),
            "{detail}"
        );
        assert!(
            detail.contains("can be normal"),
            "never says leak confirmed"
        );
        assert!(ev.iter().all(|e| e.kind != ActivityKind::ThreadGrowth));
    }

    #[test]
    fn ignores_short_noisy_or_falling_memory() {
        // Too short: nine minutes.
        let mut d = ActivityDetector::default();
        assert!(grow(&mut d, 9, |m| (400 + 100 * m) * MB, |_| 10).is_empty());
        // Saw-tooth: ends higher but keeps falling back (garbage collection).
        let mut d = ActivityDetector::default();
        let saw = |m: u64| if m.is_multiple_of(2) { 400 * MB } else { 900 * MB } + m * 20 * MB;
        let ev = grow(&mut d, 20, saw, |_| 10);
        assert!(ev.iter().all(|e| e.kind != ActivityKind::MemoryGrowth));
        // Small absolute growth is not interesting.
        let mut d = ActivityDetector::default();
        assert!(grow(&mut d, 20, |m| (100 + m) * MB, |_| 10).is_empty());
    }

    #[test]
    fn reports_thread_growth_as_a_possible_leak() {
        let mut d = ActivityDetector::default();
        let ev = grow(&mut d, 12, |_| 100 * MB, |m| 10 + 8 * m as u32);
        let hit: Vec<_> = ev
            .iter()
            .filter(|e| e.kind == ActivityKind::ThreadGrowth)
            .collect();
        assert_eq!(hit.len(), 1);
        assert!(
            hit[0]
                .detail
                .as_deref()
                .unwrap()
                .starts_with("Possible resource leak")
        );
    }

    #[test]
    fn restricted_processes_never_spike_and_floods_are_capped() {
        let mut d = ActivityDetector::default();
        let mut r = row(1, 1, 100.0, 10);
        r.restricted = true;
        d.observe(0, &[r.clone()], &CFG);
        assert!(d.observe(10_000, &[r], &CFG).is_empty());

        let many: Vec<ProcessRow> = (100..400).map(|p| row(p, 1, 0.0, 1)).collect();
        // 300 starts and 1 exit: 50 are listed, the rest are summarised.
        let out = d.observe(20_000, &many, &CFG);
        assert_eq!(out.len(), MAX_EVENTS_PER_TICK + 2);
        let summary = |k| out.iter().find(|e| e.pid.is_none() && e.kind == k);
        assert_eq!(
            summary(ActivityKind::ProcessStarted)
                .unwrap()
                .name
                .as_deref(),
            Some("250 more processes")
        );
        assert_eq!(
            summary(ActivityKind::ProcessExited)
                .unwrap()
                .name
                .as_deref(),
            Some("1 more processes")
        );
    }

    #[test]
    fn reset_prevents_a_start_flood() {
        let mut d = ActivityDetector::default();
        d.observe(0, &[row(1, 1, 0.0, 1)], &CFG);
        d.reset();
        assert!(
            d.observe(1_000, &[row(1, 1, 0.0, 1), row(2, 1, 0.0, 1)], &CFG)
                .is_empty()
        );
    }

    /// `cargo test -p tm-core --release measure_activity_diff_cost -- --ignored --nocapture`
    #[test]
    #[ignore = "measurement, not a check"]
    fn measure_activity_diff_cost() {
        let rows: Vec<ProcessRow> = (1..=1_000).map(|p| row(p, 1, 3.0, 100)).collect();
        let mut d = ActivityDetector::default();
        d.observe(0, &rows, &CFG);
        let started = std::time::Instant::now();
        let n = 1_000;
        for i in 1..=n {
            d.observe(i * 1_000, &rows, &CFG);
        }
        eprintln!(
            "activity diff, 1000 processes: {:?} per tick",
            started.elapsed() / n as u32
        );
    }

    #[test]
    fn log_numbers_events_and_serves_a_tail() {
        let mut log = ActivityLog::default();
        assert_eq!(log.last_seq(), 0);
        log.record(1, Pending::new(ActivityKind::BenchmarkStarted));
        log.record(2, Pending::new(ActivityKind::BenchmarkCompleted));
        assert_eq!(log.last_seq(), 2);
        let tail = log.since(1);
        assert_eq!(tail.len(), 1);
        assert_eq!(tail[0].seq, 2);
        for i in 0..(LOG_LEN as u64 + 10) {
            log.record(i, Pending::new(ActivityKind::AlertFired));
        }
        assert_eq!(log.since(0).len(), LOG_LEN, "bounded");
    }
}
