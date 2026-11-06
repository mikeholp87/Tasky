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
    /// Linux kernel thread (`PF_KTHREAD`); hidden by default in the UI.
    pub kernel_thread: bool,
    /// Linux: proportional set size, where shared pages are split between the
    /// processes sharing them. Only filled while the Memory page is open, and
    /// `None` for processes the system will not describe (other users').
    pub pss_bytes: Option<u64>,
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
    /// Linux `D`: uninterruptible sleep, usually waiting for I/O.
    DiskSleep,
    /// Stopped by a signal or suspended.
    Stopped,
    /// Linux `t`: stopped by a debugger.
    Tracing,
    /// Exited, waiting for its parent to collect the exit status.
    Zombie,
    Dead,
    Unknown,
}

impl From<sysinfo::ProcessStatus> for ProcessState {
    fn from(status: sysinfo::ProcessStatus) -> Self {
        use sysinfo::ProcessStatus as S;
        match status {
            S::Run => Self::Running,
            S::Sleep | S::Waking | S::Wakekill | S::Parked => Self::Sleeping,
            S::UninterruptibleDiskSleep => Self::DiskSleep,
            S::Idle => Self::Idle,
            S::Stop | S::Suspended | S::LockBlocked => Self::Stopped,
            S::Tracing => Self::Tracing,
            S::Zombie => Self::Zombie,
            S::Dead => Self::Dead,
            _ => Self::Unknown,
        }
    }
}
