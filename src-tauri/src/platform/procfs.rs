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

#[cfg(test)]
mod tests {
    use super::*;

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
