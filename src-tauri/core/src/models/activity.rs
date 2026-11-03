use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum ActivityKind {
    ProcessStarted,
    ProcessExited,
    CpuSpike,
    CpuNormal,
    MemoryJump,
    AlertFired,
    /// A suspend, resume, terminate or priority change made through this app.
    ProcessAction,
    BenchmarkStarted,
    BenchmarkCompleted,
}

/// One entry of the activity timeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ActivityEvent {
    /// Monotonic, starting at 1; lets the UI merge backfill with live events.
    pub seq: u64,
    /// Unix milliseconds.
    pub timestamp_ms: u64,
    pub kind: ActivityKind,
    pub pid: Option<u32>,
    pub name: Option<String>,
    pub detail: Option<String>,
}
