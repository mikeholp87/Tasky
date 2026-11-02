use serde::Serialize;

/// One process as shown in the process table. Sent on every tick, so it
/// carries only what the table, search and filters need.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ProcessRow {
    pub pid: u32,
    pub parent_pid: Option<u32>,
    pub name: String,
    pub status: ProcessState,
    /// Share of total machine CPU capacity (0–100), like Windows Task Manager.
    pub cpu_percent: f32,
    /// Resident set size / working set.
    pub memory_bytes: u64,
    pub virtual_memory_bytes: u64,
    pub disk_read_per_sec: f64,
    pub disk_write_per_sec: f64,
    pub user: Option<String>,
    /// Unix seconds.
    pub start_time: u64,
    pub threads: Option<u32>,
    /// Linux nice value or Windows base priority.
    pub priority: Option<i32>,
    pub exe: Option<String>,
    /// Command line joined with spaces. May contain sensitive arguments; never
    /// logged and excluded from exports unless the user opts in.
    pub cmd: Option<String>,
    pub session_id: Option<u32>,
    /// The OS refused per-process queries (protected process, other user,
    /// no elevation). CPU and disk values are unknown, not zero.
    pub restricted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum ProcessState {
    Running,
    Sleeping,
    Idle,
    /// Stopped by a signal or suspended.
    Stopped,
    Zombie,
    Dead,
    Unknown,
}

impl From<sysinfo::ProcessStatus> for ProcessState {
    fn from(status: sysinfo::ProcessStatus) -> Self {
        use sysinfo::ProcessStatus as S;
        match status {
            S::Run => Self::Running,
            S::Sleep | S::UninterruptibleDiskSleep | S::Waking | S::Wakekill | S::Parked => {
                Self::Sleeping
            }
            S::Idle => Self::Idle,
            S::Stop | S::Tracing | S::Suspended | S::LockBlocked => Self::Stopped,
            S::Zombie => Self::Zombie,
            S::Dead => Self::Dead,
            _ => Self::Unknown,
        }
    }
}
