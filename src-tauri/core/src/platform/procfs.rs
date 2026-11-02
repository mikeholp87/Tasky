//! Parsers for Linux `/proc` files. Pure functions over text so they can be
//! unit-tested on any OS.

/// Values from `/proc/meminfo`, in bytes.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct MemInfo {
    pub cached: Option<u64>,
    pub buffers: Option<u64>,
    pub s_reclaimable: Option<u64>,
}

/// Parses `/proc/meminfo`. Unknown or malformed lines are ignored.
pub fn parse_meminfo(text: &str) -> MemInfo {
    let mut info = MemInfo::default();
    for line in text.lines() {
        let Some((key, rest)) = line.split_once(':') else {
            continue;
        };
        let target = match key.trim() {
            "Cached" => &mut info.cached,
            "Buffers" => &mut info.buffers,
            "SReclaimable" => &mut info.s_reclaimable,
            _ => continue,
        };
        *target = parse_kb(rest);
    }
    info
}

/// Parses a meminfo value such as `"   123456 kB"` into bytes.
fn parse_kb(value: &str) -> Option<u64> {
    let mut parts = value.split_whitespace();
    let number: u64 = parts.next()?.parse().ok()?;
    match parts.next() {
        Some("kB") | None => number.checked_mul(1024),
        Some(_) => None,
    }
}

/// Selected fields of `/proc/[pid]/stat`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stat {
    pub state: char,
    pub ppid: u32,
    pub nice: i32,
    pub num_threads: u32,
}

/// Parses `/proc/[pid]/stat`. The command name (field 2) is wrapped in
/// parentheses and may itself contain spaces or `)`, so fields are counted
/// from the *last* closing parenthesis.
pub fn parse_stat(text: &str) -> Option<Stat> {
    let rest = &text[text.rfind(')')? + 1..];
    let fields: Vec<&str> = rest.split_whitespace().collect();
    // After the comm field, index 0 is field 3 (state); field N is at N - 3.
    let field = |n: usize| fields.get(n - 3).copied();
    Some(Stat {
        state: field(3)?.chars().next()?,
        ppid: field(4)?.parse().ok()?,
        nice: field(19)?.parse().ok()?,
        num_threads: field(20)?.parse().ok()?,
    })
}

/// One socket line from `/proc/net/{tcp,tcp6,udp,udp6}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetEntry {
    pub local: (std::net::IpAddr, u16),
    pub remote: (std::net::IpAddr, u16),
    /// Kernel TCP state code (`0A` = LISTEN, `01` = ESTABLISHED, ...).
    pub state: u8,
    pub inode: u64,
}

/// Parses a data line of `/proc/net/tcp*` or `/proc/net/udp*`.
/// Fields: `sl local rem st tx:rx tr:when retrnsmt uid timeout inode ...`.
pub fn parse_net_line(line: &str) -> Option<NetEntry> {
    let f: Vec<&str> = line.split_whitespace().collect();
    if f.len() < 10 || !f[0].ends_with(':') {
        return None; // header or malformed
    }
    Some(NetEntry {
        local: parse_hex_endpoint(f[1])?,
        remote: parse_hex_endpoint(f[2])?,
        state: u8::from_str_radix(f[3], 16).ok()?,
        inode: f[9].parse().ok()?,
    })
}

/// `0100007F:1F90` → 127.0.0.1:8080. Addresses are 32-bit words in host
/// (little-endian) order; the port is big-endian hex.
fn parse_hex_endpoint(s: &str) -> Option<(std::net::IpAddr, u16)> {
    use std::net::{Ipv4Addr, Ipv6Addr};
    let (addr, port) = s.split_once(':')?;
    let port = u16::from_str_radix(port, 16).ok()?;
    let ip = match addr.len() {
        8 => {
            let word = u32::from_str_radix(addr, 16).ok()?;
            Ipv4Addr::from(word.to_le_bytes()).into()
        }
        32 => {
            let mut bytes = [0u8; 16];
            for i in 0..4 {
                let word = u32::from_str_radix(addr.get(i * 8..i * 8 + 8)?, 16).ok()?;
                bytes[i * 4..i * 4 + 4].copy_from_slice(&word.to_le_bytes());
            }
            Ipv6Addr::from(bytes).into()
        }
        _ => return None,
    };
    Some((ip, port))
}

/// Socket inode from an `/proc/[pid]/fd/*` link target such as `socket:[12345]`.
pub fn socket_inode(link: &str) -> Option<u64> {
    link.strip_prefix("socket:[")?
        .strip_suffix(']')?
        .parse()
        .ok()
}

/// Architecture from the first 20 bytes of an ELF file (`e_machine`).
pub fn elf_architecture(header: &[u8]) -> Option<&'static str> {
    if header.len() < 20 || &header[..4] != b"\x7fELF" {
        return None;
    }
    // EI_DATA (byte 5): 1 = little endian, 2 = big endian.
    let machine = match header[5] {
        1 => u16::from_le_bytes([header[18], header[19]]),
        2 => u16::from_be_bytes([header[18], header[19]]),
        _ => return None,
    };
    Some(match machine {
        0x3E => "x86_64",
        0x03 => "x86",
        0xB7 => "aarch64",
        0x28 => "arm",
        0xF3 => "riscv",
        _ => "unknown",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ipv4_tcp_line() {
        let line = "   0: 0100007F:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 34567 1 0000000000000000 100 0 0 10 0";
        let e = parse_net_line(line).unwrap();
        assert_eq!(e.local, ("127.0.0.1".parse().unwrap(), 8080));
        assert_eq!(e.remote, ("0.0.0.0".parse().unwrap(), 0));
        assert_eq!(e.state, 0x0A);
        assert_eq!(e.inode, 34567);
    }

    #[test]
    fn parses_ipv6_loopback() {
        let line = "   1: 00000000000000000000000001000000:0016 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 999 1 0 100 0 0 10 0";
        let e = parse_net_line(line).unwrap();
        assert_eq!(e.local, ("::1".parse().unwrap(), 22));
    }

    #[test]
    fn skips_header_lines() {
        assert_eq!(
            parse_net_line(
                "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode"
            ),
            None
        );
    }

    #[test]
    fn extracts_socket_inodes() {
        assert_eq!(socket_inode("socket:[12345]"), Some(12345));
        assert_eq!(socket_inode("pipe:[1]"), None);
        assert_eq!(socket_inode("/dev/null"), None);
    }

    #[test]
    fn reads_elf_machine() {
        let mut h = [0u8; 20];
        h[..4].copy_from_slice(b"\x7fELF");
        h[5] = 1;
        h[18] = 0x3E;
        assert_eq!(elf_architecture(&h), Some("x86_64"));
        assert_eq!(elf_architecture(b"MZ not elf at all...."), None);
    }

    #[test]
    fn parses_stat_with_tricky_command_name() {
        let text = "1234 (my (weird) proc) S 1 1234 1234 0 -1 4194560 100 0 0 0 \
                    5 3 0 0 20 -5 7 0 12345 1000000 250 18446744073709551615";
        let stat = parse_stat(text).unwrap();
        assert_eq!(
            stat,
            Stat {
                state: 'S',
                ppid: 1,
                nice: -5,
                num_threads: 7
            }
        );
    }

    #[test]
    fn rejects_truncated_stat() {
        assert_eq!(parse_stat("1 (init) S 0 1"), None);
        assert_eq!(parse_stat("garbage"), None);
    }

    const SAMPLE: &str = "MemTotal:       32765432 kB\n\
MemFree:         1234567 kB\n\
Buffers:          204800 kB\n\
Cached:          8388608 kB\n\
SwapCached:            0 kB\n\
SReclaimable:     524288 kB\n\
garbage line\n";

    #[test]
    fn parses_cache_fields_in_bytes() {
        let m = parse_meminfo(SAMPLE);
        assert_eq!(m.buffers, Some(204_800 * 1024));
        assert_eq!(m.cached, Some(8_388_608 * 1024));
        assert_eq!(m.s_reclaimable, Some(524_288 * 1024));
    }

    #[test]
    fn missing_fields_are_none() {
        let m = parse_meminfo("MemTotal: 1 kB\n");
        assert_eq!(m, MemInfo::default());
    }

    #[test]
    fn rejects_unknown_units() {
        assert_eq!(parse_kb(" 12 MB"), None);
        assert_eq!(parse_kb(" abc kB"), None);
    }
}
