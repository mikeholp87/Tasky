//! Per-process sockets via the IP Helper API.
//! <https://learn.microsoft.com/windows/win32/api/iphlpapi/nf-iphlpapi-getextendedtcptable>

use std::net::{Ipv4Addr, Ipv6Addr};

use ::windows::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, NO_ERROR};
use ::windows::Win32::NetworkManagement::IpHelper::{
    GetExtendedTcpTable, GetExtendedUdpTable, MIB_TCP6ROW_OWNER_PID, MIB_TCPROW_OWNER_PID,
    MIB_UDP6ROW_OWNER_PID, MIB_UDPROW_OWNER_PID, TCP_TABLE_OWNER_PID_ALL, UDP_TABLE_OWNER_PID,
};
use ::windows::Win32::Networking::WinSock::{AF_INET, AF_INET6};

use crate::models::{Connection, Protocol, TcpState};

/// All TCP/UDP sockets owned by `pid`. Returns `None` if the tables cannot be read.
pub fn connections(pid: u32) -> Option<Vec<Connection>> {
    Some(
        owned_sockets()?
            .into_iter()
            .filter(|(owner, _)| *owner == pid)
            .map(|(_, c)| c)
            .collect(),
    )
}

/// Every TCP/UDP socket on the machine with its owning PID (0 when the
/// system attributes it to nobody, e.g. sockets in TIME_WAIT).
pub fn owned_sockets() -> Option<Vec<(u32, Connection)>> {
    let mut out = Vec::new();

    for row in table::<MIB_TCPROW_OWNER_PID>(Kind::Tcp, AF_INET.0.into())? {
        let state = tcp_state(row.dwState);
        let listening = state == TcpState::Listen;
        out.push((
            row.dwOwningPid,
            Connection {
                protocol: Protocol::Tcp,
                local_address: ipv4(row.dwLocalAddr).to_string(),
                local_port: port(row.dwLocalPort),
                remote_address: (!listening).then(|| ipv4(row.dwRemoteAddr).to_string()),
                remote_port: (!listening).then(|| port(row.dwRemotePort)),
                state: Some(state),
            },
        ));
    }
    for row in table::<MIB_TCP6ROW_OWNER_PID>(Kind::Tcp, AF_INET6.0.into())? {
        let state = tcp_state(row.dwState);
        let listening = state == TcpState::Listen;
        out.push((
            row.dwOwningPid,
            Connection {
                protocol: Protocol::Tcp6,
                local_address: Ipv6Addr::from(row.ucLocalAddr).to_string(),
                local_port: port(row.dwLocalPort),
                remote_address: (!listening).then(|| Ipv6Addr::from(row.ucRemoteAddr).to_string()),
                remote_port: (!listening).then(|| port(row.dwRemotePort)),
                state: Some(state),
            },
        ));
    }
    for row in table::<MIB_UDPROW_OWNER_PID>(Kind::Udp, AF_INET.0.into())? {
        out.push((
            row.dwOwningPid,
            udp(
                Protocol::Udp,
                ipv4(row.dwLocalAddr).to_string(),
                row.dwLocalPort,
            ),
        ));
    }
    for row in table::<MIB_UDP6ROW_OWNER_PID>(Kind::Udp, AF_INET6.0.into())? {
        let addr = Ipv6Addr::from(row.ucLocalAddr).to_string();
        out.push((row.dwOwningPid, udp(Protocol::Udp6, addr, row.dwLocalPort)));
    }
    Some(out)
}

fn udp(protocol: Protocol, local_address: String, raw_port: u32) -> Connection {
    Connection {
        protocol,
        local_address,
        local_port: port(raw_port),
        remote_address: None,
        remote_port: None,
        state: None,
    }
}

#[derive(Clone, Copy)]
enum Kind {
    Tcp,
    Udp,
}

/// Fetches a table, retrying while it grows between the size query and read.
/// Rows follow a leading `u32` entry count.
fn table<Row: Copy>(kind: Kind, family: u32) -> Option<Vec<Row>> {
    let mut size = 0u32;
    for _ in 0..4 {
        // u32 backing keeps the buffer aligned for the row structs.
        let mut buf = vec![0u32; (size as usize).div_ceil(4).max(1)];
        let ptr = Some(buf.as_mut_ptr().cast());
        // SAFETY: `buf` holds at least `size` bytes; the API writes at most that.
        let rc = unsafe {
            match kind {
                Kind::Tcp => {
                    GetExtendedTcpTable(ptr, &mut size, false, family, TCP_TABLE_OWNER_PID_ALL, 0)
                }
                Kind::Udp => {
                    GetExtendedUdpTable(ptr, &mut size, false, family, UDP_TABLE_OWNER_PID, 0)
                }
            }
        };
        if rc == ERROR_INSUFFICIENT_BUFFER.0 {
            continue;
        }
        if rc != NO_ERROR.0 {
            tracing::debug!("GetExtended*Table failed: {rc}");
            return None;
        }
        let count = buf[0] as usize;
        let row_bytes = size_of::<Row>();
        // Rows start after the count, padded to the row alignment.
        let offset = align_of::<Row>().max(4);
        if offset + count * row_bytes > buf.len() * 4 {
            return None;
        }
        // SAFETY: bounds checked above; Row is a plain-old-data FFI struct.
        let rows = unsafe {
            let base = buf.as_ptr().cast::<u8>().add(offset).cast::<Row>();
            std::slice::from_raw_parts(base, count).to_vec()
        };
        return Some(rows);
    }
    None
}

/// Ports are stored in network byte order in the low 16 bits.
fn port(raw: u32) -> u16 {
    u16::from_be(raw as u16)
}

/// IPv4 addresses are stored in network byte order.
fn ipv4(raw: u32) -> Ipv4Addr {
    Ipv4Addr::from(raw.to_ne_bytes())
}

/// `MIB_TCP_STATE` values.
fn tcp_state(raw: u32) -> TcpState {
    match raw {
        1 | 12 => TcpState::Closed,
        2 => TcpState::Listen,
        3 => TcpState::SynSent,
        4 => TcpState::SynReceived,
        5 => TcpState::Established,
        6 => TcpState::FinWait1,
        7 => TcpState::FinWait2,
        8 => TcpState::CloseWait,
        9 => TcpState::Closing,
        10 => TcpState::LastAck,
        11 => TcpState::TimeWait,
        _ => TcpState::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn decodes_byte_order() {
        // Port 8080 in network order, as stored by Windows on little-endian.
        assert_eq!(port(u32::from(8080u16.to_be())), 8080);
        assert_eq!(
            ipv4(u32::from_ne_bytes([127, 0, 0, 1])),
            Ipv4Addr::LOCALHOST
        );
    }

    /// Integration: a listener opened by this test shows up for our PID.
    #[test]
    fn finds_own_listening_socket() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let bound = listener.local_addr().unwrap().port();
        let conns = connections(std::process::id()).expect("tables readable");
        assert!(
            conns
                .iter()
                .any(|c| c.local_port == bound && c.state == Some(TcpState::Listen)),
            "listener on port {bound} not found in {conns:?}"
        );
    }
}
