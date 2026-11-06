//! Rate calculations based on monotonically increasing OS counters.
//!
//! Rates are always derived from the *measured* elapsed time between two
//! samples, never from the nominal polling interval: timers drift, the engine
//! may be paused, and the OS may schedule us late.

use std::time::Duration;

/// Bytes (or operations) per second between two counter readings.
///
/// Returns `0.0` when the counter went backwards (interface reset, counter
/// wrap, device re-attached) or no time has elapsed, rather than reporting a
/// huge spurious spike.
pub fn counter_rate(previous: u64, current: u64, elapsed: Duration) -> f64 {
    let secs = elapsed.as_secs_f64();
    if secs <= f64::EPSILON || current < previous {
        return 0.0;
    }
    (current - previous) as f64 / secs
}

/// Rate from a delta that the OS (or sysinfo) already computed since the last
/// refresh.
pub fn delta_rate(delta: u64, elapsed: Duration) -> f64 {
    let secs = elapsed.as_secs_f64();
    if secs <= f64::EPSILON {
        return 0.0;
    }
    delta as f64 / secs
}

/// Percentage `part / total * 100`, clamped to `0..=100`. Returns 0 for an
/// empty total instead of NaN.
pub fn percent(part: u64, total: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    ((part as f64 / total as f64) * 100.0).clamp(0.0, 100.0)
}

/// Share of time a disk was busy between two samples, 0–100.
///
/// Uses the OS clock when one is provided with the counters (Windows idle
/// time + query time); otherwise busy time over the measured elapsed time
/// (Linux). Returns `None` when the OS gives no busy/idle information.
pub fn active_percent(
    prev: &crate::platform::DiskIoCounters,
    cur: &crate::platform::DiskIoCounters,
    elapsed: Duration,
) -> Option<f64> {
    let ratio = match (prev.idle_ns, cur.idle_ns, prev.clock_ns, cur.clock_ns) {
        (Some(pi), Some(ci), Some(pc), Some(cc)) => {
            let clock = cc.checked_sub(pc)? as f64;
            if clock <= 0.0 {
                return None;
            }
            1.0 - ci.checked_sub(pi)? as f64 / clock
        }
        _ => {
            let busy = cur.busy_ns?.checked_sub(prev.busy_ns?)? as f64;
            let wall = elapsed.as_nanos() as f64;
            if wall <= 0.0 {
                return None;
            }
            busy / wall
        }
    };
    Some((ratio * 100.0).clamp(0.0, 100.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_actual_elapsed_time() {
        // 3000 bytes over 1.5 s is 2000 B/s, not 3000 B/s.
        let r = counter_rate(1_000, 4_000, Duration::from_millis(1_500));
        assert!((r - 2_000.0).abs() < 1e-9);
    }

    #[test]
    fn counter_reset_yields_zero_not_spike() {
        assert_eq!(counter_rate(10_000, 50, Duration::from_secs(1)), 0.0);
    }

    #[test]
    fn zero_elapsed_is_safe() {
        assert_eq!(counter_rate(0, 100, Duration::ZERO), 0.0);
        assert_eq!(delta_rate(100, Duration::ZERO), 0.0);
    }

    #[test]
    fn delta_rate_scales_by_elapsed() {
        assert!((delta_rate(500, Duration::from_millis(500)) - 1_000.0).abs() < 1e-9);
    }

    #[test]
    fn percent_handles_edges() {
        assert_eq!(percent(1, 0), 0.0);
        assert_eq!(percent(50, 200), 25.0);
        assert_eq!(percent(300, 200), 100.0);
    }
}

#[cfg(test)]
mod disk_tests {
    use super::*;
    use crate::platform::DiskIoCounters;

    #[test]
    fn active_from_idle_and_clock() {
        let prev = DiskIoCounters {
            idle_ns: Some(1_000),
            clock_ns: Some(10_000),
            ..Default::default()
        };
        let cur = DiskIoCounters {
            idle_ns: Some(1_750),
            clock_ns: Some(11_000),
            ..Default::default()
        };
        // 750 of 1000 ns idle → 25% active.
        assert_eq!(
            active_percent(&prev, &cur, Duration::from_secs(1)),
            Some(25.0)
        );
    }

    #[test]
    fn active_from_busy_time_over_elapsed() {
        let prev = DiskIoCounters {
            busy_ns: Some(0),
            ..Default::default()
        };
        let cur = DiskIoCounters {
            busy_ns: Some(500_000_000),
            ..Default::default()
        };
        assert_eq!(
            active_percent(&prev, &cur, Duration::from_secs(2)),
            Some(25.0)
        );
    }

    #[test]
    fn active_handles_resets_and_missing_data() {
        let none = DiskIoCounters::default();
        assert_eq!(active_percent(&none, &none, Duration::from_secs(1)), None);
        let prev = DiskIoCounters {
            busy_ns: Some(900),
            ..Default::default()
        };
        let reset = DiskIoCounters {
            busy_ns: Some(10),
            ..Default::default()
        };
        assert_eq!(active_percent(&prev, &reset, Duration::from_secs(1)), None);
        let prev = DiskIoCounters {
            idle_ns: Some(100),
            clock_ns: Some(100),
            ..Default::default()
        };
        let reset = DiskIoCounters {
            idle_ns: Some(1),
            clock_ns: Some(200),
            ..Default::default()
        };
        assert_eq!(active_percent(&prev, &reset, Duration::from_secs(1)), None);
    }
}
