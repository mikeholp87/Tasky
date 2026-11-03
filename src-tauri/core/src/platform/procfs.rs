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

/// Memory totals from `/proc/[pid]/smaps_rollup`, in bytes.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SmapsRollup {
    pub rss: Option<u64>,
    /// Proportional set size: private pages plus each shared page divided by
    /// the number of processes sharing it. Summing PSS over processes does
    /// not count shared memory twice, unlike RSS.
    pub pss: Option<u64>,
}

/// Parses `/proc/[pid]/smaps_rollup`. Lines are `Key:   123 kB`; unknown or
/// malformed lines are ignored.
pub fn parse_smaps_rollup(text: &str) -> SmapsRollup {
    let mut out = SmapsRollup::default();
    for line in text.lines() {
        let Some((key, rest)) = line.split_once(':') else {
            continue;
        };
        match key {
            "Rss" => out.rss = parse_kb(rest),
            "Pss" => out.pss = parse_kb(rest),
            _ => {}
        }
    }
    out
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

/// One line of `/proc/[pid]/maps`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapsEntry {
    pub start: u64,
    pub end: u64,
    /// Four characters, e.g. `r-xp` (last is `p` private or `s` shared).
    pub perms: String,
    pub offset: u64,
    /// Backing file or a bracketed pseudo-name such as `[heap]`; `None` for
    /// anonymous memory. May end in ` (deleted)`.
    pub path: Option<String>,
}

/// Parses `address perms offset dev inode  pathname`. The pathname is the
/// rest of the line and may contain spaces.
pub fn parse_maps_line(line: &str) -> Option<MapsEntry> {
    fn field<'a>(rest: &mut &'a str) -> Option<&'a str> {
        let trimmed = rest.trim_start();
        if trimmed.is_empty() {
            return None;
        }
        let end = trimmed.find(char::is_whitespace).unwrap_or(trimmed.len());
        let (head, tail) = trimmed.split_at(end);
        *rest = tail;
        Some(head)
    }
    let mut rest = line;
    let range = field(&mut rest)?;
    let perms = field(&mut rest)?;
    let offset = field(&mut rest)?;
    let _dev = field(&mut rest)?;
    let _inode = field(&mut rest)?;
    let (start, end) = range.split_once('-')?;
    if perms.len() != 4 {
        return None;
    }
    let path = rest.trim();
    Some(MapsEntry {
        start: u64::from_str_radix(start, 16).ok()?,
        end: u64::from_str_radix(end, 16).ok()?,
        perms: perms.to_owned(),
        offset: u64::from_str_radix(offset, 16).ok()?,
        path: (!path.is_empty()).then(|| path.to_owned()),
    })
}

/// Access mode from `/proc/[pid]/fdinfo/[fd]`: the `flags:` line is octal
/// and its low two bits are `O_RDONLY`, `O_WRONLY` or `O_RDWR`.
pub fn parse_fdinfo_access(text: &str) -> Option<crate::models::FileAccess> {
    use crate::models::FileAccess;
    let flags = text
        .lines()
        .find_map(|l| l.strip_prefix("flags:"))
        .and_then(|v| u32::from_str_radix(v.trim(), 8).ok())?;
    match flags & 0o3 {
        0 => Some(FileAccess::Read),
        1 => Some(FileAccess::Write),
        2 => Some(FileAccess::ReadWrite),
        _ => None,
    }
}

/// Kind of an `/proc/[pid]/fd/*` link target. Paths are reported as `File`;
/// the caller refines directories with a metadata call.
pub fn classify_fd_target(target: &str) -> crate::models::OpenFileKind {
    use crate::models::OpenFileKind;
    if target.starts_with("socket:[") {
        OpenFileKind::Socket
    } else if target.starts_with("pipe:[") {
        OpenFileKind::Pipe
    } else if target.starts_with("anon_inode:") || target.starts_with("/memfd:") {
        OpenFileKind::Anonymous
    } else if target.starts_with("/dev/") && !target.starts_with("/dev/shm/") {
        OpenFileKind::Device
    } else if target.starts_with('/') {
        OpenFileKind::File
    } else {
        OpenFileKind::Other
    }
}

/// Whether the file at `target` (an fd link or maps path) is the searched
/// `query`, or beneath it when `query_is_dir`. A trailing ` (deleted)` marker
/// is ignored. Paths must already be canonical.
pub fn path_matches(target: &str, query: &str, query_is_dir: bool) -> bool {
    let target = target.strip_suffix(" (deleted)").unwrap_or(target);
    if target == query {
        return true;
    }
    query_is_dir
        && target
            .strip_prefix(query)
            .is_some_and(|rest| query == "/" || rest.starts_with('/'))
}

/// Selected fields of `/proc/[pid]/task/[tid]/stat`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskStat {
    pub comm: String,
    pub state: char,
    /// User + system CPU time in clock ticks.
    pub cpu_ticks: u64,
    pub nice: i32,
    /// Start time in clock ticks since boot.
    pub start_ticks: u64,
    pub processor: u32,
}

/// Parses a task `stat` line. Like [`parse_stat`], fields are counted from
/// the last `)` because the command name can contain anything.
pub fn parse_task_stat(text: &str) -> Option<TaskStat> {
    let open = text.find('(')?;
    let close = text.rfind(')')?;
    let comm = text.get(open + 1..close)?.to_owned();
    let fields: Vec<&str> = text[close + 1..].split_whitespace().collect();
    let field = |n: usize| fields.get(n - 3).copied();
    Some(TaskStat {
        comm,
        state: field(3)?.chars().next()?,
        cpu_ticks: field(14)?
            .parse::<u64>()
            .ok()?
            .saturating_add(field(15)?.parse().ok()?),
        nice: field(19)?.parse().ok()?,
        start_ticks: field(22)?.parse().ok()?,
        processor: field(39)?.parse().ok()?,
    })
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

/// Cumulative counters for one block device from `/proc/diskstats`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiskStat {
    pub reads_completed: u64,
    pub writes_completed: u64,
    /// Milliseconds the device spent doing I/O (field 13).
    pub io_ms: u64,
}

/// Parses `/proc/diskstats` into device name → counters.
/// Line format: `major minor name reads merged sectors ms writes merged sectors ms in_flight io_ms ...`.
pub fn parse_diskstats(text: &str) -> std::collections::HashMap<String, DiskStat> {
    text.lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split_whitespace().collect();
            if f.len() < 13 {
                return None;
            }
            Some((
                f[2].to_owned(),
                DiskStat {
                    reads_completed: f[3].parse().ok()?,
                    writes_completed: f[7].parse().ok()?,
                    io_ms: f[12].parse().ok()?,
                },
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_smaps_rollup() {
        let text = "55d0c8a00000-7ffd1a3c5000 ---p 00000000 00:00 0                          [rollup]\n\
                    Rss:              204800 kB\n\
                    Pss:              102400 kB\n\
                    Pss_Anon:          51200 kB\n\
                    Shared_Clean:      80000 kB\n\
                    Bogus line\n";
        assert_eq!(
            parse_smaps_rollup(text),
            SmapsRollup {
                rss: Some(204_800 * 1024),
                pss: Some(102_400 * 1024)
            }
        );
        assert_eq!(parse_smaps_rollup(""), SmapsRollup::default());
        assert_eq!(parse_smaps_rollup("Pss: lots kB").pss, None);
    }

    #[test]
    fn parses_diskstats() {
        let text = " 259       0 nvme0n1 120 3 9000 50 300 7 4000 80 0 210 130 0 0 0 0\n\
                    259       2 nvme0n1p2 100 1 8000 40 250 5 3500 70 2 190 110\n\
                    garbage";
        let stats = parse_diskstats(text);
        assert_eq!(
            stats.get("nvme0n1p2"),
            Some(&DiskStat {
                reads_completed: 100,
                writes_completed: 250,
                io_ms: 190
            })
        );
        assert_eq!(stats.len(), 2);
    }

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
    fn parses_maps_lines() {
        let e = parse_maps_line(
            "7f3a1c000000-7f3a1c1c5000 r-xp 00001000 fd:01 1234     /usr/lib/x86_64-linux-gnu/libc.so.6",
        )
        .unwrap();
        assert_eq!(
            (e.start, e.end, e.offset),
            (0x7f3a_1c00_0000, 0x7f3a_1c1c_5000, 0x1000)
        );
        assert_eq!(e.perms, "r-xp");
        assert_eq!(
            e.path.as_deref(),
            Some("/usr/lib/x86_64-linux-gnu/libc.so.6")
        );

        let anon = parse_maps_line("7ffd1a3c5000-7ffd1a3e6000 rw-p 00000000 00:00 0 ").unwrap();
        assert_eq!(anon.path, None);

        let spaced = parse_maps_line(
            "55d0c8a00000-55d0c8a01000 r--p 00000000 08:02 99   /home/me/My Apps/tool (deleted)",
        )
        .unwrap();
        assert_eq!(
            spaced.path.as_deref(),
            Some("/home/me/My Apps/tool (deleted)")
        );

        let heap =
            parse_maps_line("55d0c9000000-55d0c9021000 rw-p 00000000 00:00 0   [heap]").unwrap();
        assert_eq!(heap.path.as_deref(), Some("[heap]"));

        assert_eq!(parse_maps_line("garbage"), None);
        assert_eq!(parse_maps_line("zz-zz r-xp 0 0:0 0"), None);
    }

    #[test]
    fn reads_fd_access_from_octal_flags() {
        use crate::models::FileAccess;
        assert_eq!(
            parse_fdinfo_access("pos:\t0\nflags:\t0100002\nmnt_id:\t29\n"),
            Some(FileAccess::ReadWrite)
        );
        assert_eq!(
            parse_fdinfo_access("flags:\t02000000\n"),
            Some(FileAccess::Read)
        );
        assert_eq!(parse_fdinfo_access("flags:\t01\n"), Some(FileAccess::Write));
        assert_eq!(parse_fdinfo_access("pos: 0\n"), None);
    }

    #[test]
    fn classifies_fd_targets() {
        use crate::models::OpenFileKind as K;
        assert_eq!(classify_fd_target("socket:[1]"), K::Socket);
        assert_eq!(classify_fd_target("pipe:[2]"), K::Pipe);
        assert_eq!(classify_fd_target("anon_inode:[eventpoll]"), K::Anonymous);
        assert_eq!(classify_fd_target("/memfd:x (deleted)"), K::Anonymous);
        assert_eq!(classify_fd_target("/dev/null"), K::Device);
        assert_eq!(classify_fd_target("/dev/shm/seg"), K::File);
        assert_eq!(classify_fd_target("/home/a/b.txt"), K::File);
        assert_eq!(classify_fd_target("weird"), K::Other);
    }

    #[test]
    fn matches_files_and_directories() {
        assert!(path_matches("/a/b.txt", "/a/b.txt", false));
        assert!(path_matches("/a/b.txt (deleted)", "/a/b.txt", false));
        assert!(!path_matches("/a/b.txt2", "/a/b.txt", false));
        assert!(path_matches("/a/b/c", "/a/b", true));
        assert!(path_matches("/a/b", "/a/b", true));
        assert!(
            !path_matches("/a/bc", "/a/b", true),
            "prefix is not a parent"
        );
        assert!(
            !path_matches("/a/b/c", "/a/b", false),
            "file search is exact"
        );
        assert!(path_matches("/anything", "/", true));
    }

    #[test]
    fn parses_task_stat() {
        // Fields 3.. after the command name; set the ones we read.
        let mut f = vec!["0".to_string(); 50];
        let mut set = |n: usize, v: &str| f[n - 3] = v.to_string();
        set(3, "S");
        set(14, "120");
        set(15, "30");
        set(19, "-5");
        set(22, "5000");
        set(39, "3");
        let text = format!("77 (Web Content (x)) {}", f.join(" "));
        assert_eq!(
            parse_task_stat(&text),
            Some(TaskStat {
                comm: "Web Content (x)".into(),
                state: 'S',
                cpu_ticks: 150,
                nice: -5,
                start_ticks: 5000,
                processor: 3,
            })
        );
        assert_eq!(parse_task_stat("1 (a) S 1"), None);
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
