use std::collections::HashSet;
use std::fs;

use crate::models::{Connection, Protocol, TcpState};
use crate::platform::procfs::{self, NetEntry};

/// Sockets owned by `pid`: collect its socket inodes from `/proc/[pid]/fd`,
/// then match them against `/proc/net/*`.
///
/// Returns `None` when the fd directory is unreadable (another user's
/// process without root), so the UI can say "unavailable" instead of
/// claiming the process has no connections.
pub fn connections(pid: u32) -> Option<Vec<Connection>> {
    let fds = fs::read_dir(format!("/proc/{pid}/fd")).ok()?;
    let inodes: HashSet<u64> = fds
        .filter_map(|e| e.ok())
        .filter_map(|e| fs::read_link(e.path()).ok())
        .filter_map(|target| procfs::socket_inode(&target.to_string_lossy()))
        .collect();
    if inodes.is_empty() {
        return Some(Vec::new());
    }

    let mut out = Vec::new();
    for (file, protocol) in [
        ("tcp", Protocol::Tcp),
        ("tcp6", Protocol::Tcp6),
        ("udp", Protocol::Udp),
        ("udp6", Protocol::Udp6),
    ] {
        let Ok(text) = fs::read_to_string(format!("/proc/{pid}/net/{file}")) else {
            continue;
        };
        out.extend(
            text.lines()
                .filter_map(procfs::parse_net_line)
                .filter(|e| inodes.contains(&e.inode))
                .map(|e| to_connection(&e, protocol)),
        );
    }
    Some(out)
}

pub(super) fn to_connection(e: &NetEntry, protocol: Protocol) -> Connection {
    let is_tcp = matches!(protocol, Protocol::Tcp | Protocol::Tcp6);
    let state = is_tcp.then(|| tcp_state(e.state));
    let has_remote = is_tcp && state != Some(TcpState::Listen) && e.remote.1 != 0;
    Connection {
        protocol,
        local_address: e.local.0.to_string(),
        local_port: e.local.1,
        remote_address: has_remote.then(|| e.remote.0.to_string()),
        remote_port: has_remote.then_some(e.remote.1),
        state,
    }
}

/// Kernel TCP states from `include/net/tcp_states.h`.
fn tcp_state(code: u8) -> TcpState {
    match code {
        0x01 => TcpState::Established,
        0x02 => TcpState::SynSent,
        0x03 => TcpState::SynReceived,
        0x04 => TcpState::FinWait1,
        0x05 => TcpState::FinWait2,
        0x06 => TcpState::TimeWait,
        0x07 => TcpState::Closed,
        0x08 => TcpState::CloseWait,
        0x09 => TcpState::LastAck,
        0x0A => TcpState::Listen,
        0x0B => TcpState::Closing,
        _ => TcpState::Unknown,
    }
}
