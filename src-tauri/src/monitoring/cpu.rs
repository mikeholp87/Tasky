use sysinfo::{CpuRefreshKind, System};

use crate::models::{CpuSnapshot, LoadAverage};
use crate::platform;

/// What the engine refreshes for CPUs on every tick.
pub fn refresh_kind() -> CpuRefreshKind {
    CpuRefreshKind::nothing().with_usage().with_frequency()
}

pub fn snapshot(sys: &System) -> CpuSnapshot {
    let cpus = sys.cpus();
    let per_core: Vec<f32> = cpus.iter().map(|c| sanitize(c.usage())).collect();
    CpuSnapshot {
        usage_percent: sanitize(sys.global_cpu_usage()),
        per_core,
        frequency_mhz: average_frequency(cpus.iter().map(|c| c.frequency())),
        load_average: load_average(),
    }
}

fn load_average() -> Option<LoadAverage> {
    if !platform::HAS_LOAD_AVERAGE {
        return None;
    }
    System::load_average().ok().map(|l| LoadAverage {
        one: l.one,
        five: l.five,
        fifteen: l.fifteen,
    })
}

/// Mean of non-zero frequencies; `None` when the OS reports nothing.
pub fn average_frequency(freqs: impl Iterator<Item = u64>) -> Option<u64> {
    let (sum, count) = freqs
        .filter(|&f| f > 0)
        .fold((0u64, 0u64), |(s, c), f| (s.saturating_add(f), c + 1));
    (count > 0).then(|| sum / count)
}

/// Clamps to 0..=100 and replaces NaN (seen transiently on some systems).
fn sanitize(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(0.0, 100.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn average_ignores_zero_readings() {
        assert_eq!(average_frequency([0, 3000, 4000].into_iter()), Some(3500));
        assert_eq!(average_frequency([0, 0].into_iter()), None);
        assert_eq!(average_frequency(std::iter::empty()), None);
    }

    #[test]
    fn sanitize_clamps_and_drops_nan() {
        assert_eq!(sanitize(f32::NAN), 0.0);
        assert_eq!(sanitize(120.0), 100.0);
        assert_eq!(sanitize(-1.0), 0.0);
        assert_eq!(sanitize(42.5), 42.5);
    }
}
