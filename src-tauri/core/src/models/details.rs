use serde::Serialize;

use super::ProcessRow;

/// Everything the Process Details page shows. Fetched on demand for one PID;
/// live values keep updating from regular ticks.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ProcessDetails {
    pub row: ProcessRow,
    pub parent_name: Option<String>,
    pub cwd: Option<String>,
    /// e.g. "x64", "x86", "arm64".
    pub architecture: Option<String>,
    /// Lifetime bytes read/written, where the OS reports them.
    pub total_read_bytes: Option<u64>,
    pub total_written_bytes: Option<u64>,
    /// `None` when connections cannot be listed for this process/platform.
    pub connections: Option<Vec<Connection>>,
    pub history: Vec<ProcessPoint>,
}

/// Per-process sample kept in a short ring buffer.
#[derive(Debug, Clone, Copy, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ProcessPoint {
    /// Unix milliseconds.
    pub t: u64,
    pub cpu: f32,
    pub memory: u64,
    pub read: f64,
    pub write: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub protocol: Protocol,
    pub local_address: String,
    pub local_port: u16,
    pub remote_address: Option<String>,
    pub remote_port: Option<u16>,
    /// TCP only.
    pub state: Option<TcpState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum Protocol {
    Tcp,
    Tcp6,
    Udp,
    Udp6,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum TcpState {
    Listen,
    SynSent,
    SynReceived,
    Established,
    FinWait1,
    FinWait2,
    CloseWait,
    Closing,
    LastAck,
    TimeWait,
    Closed,
    Unknown,
}
