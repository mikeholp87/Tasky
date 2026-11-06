use sysinfo::{MemoryRefreshKind, System};

use crate::models::MemorySnapshot;
use crate::monitoring::rates::percent;
use crate::platform::MemoryExtras;

pub fn refresh_kind() -> MemoryRefreshKind {
    MemoryRefreshKind::nothing().with_ram().with_swap()
}

pub fn snapshot(sys: &System, extras: MemoryExtras) -> MemorySnapshot {
    build(
        sys.total_memory(),
        sys.used_memory(),
        sys.available_memory(),
        sys.total_swap(),
        sys.used_swap(),
        extras,
    )
}

fn build(
    total: u64,
    used: u64,
    available: u64,
    swap_total: u64,
    swap_used: u64,
    extras: MemoryExtras,
) -> MemorySnapshot {
    MemorySnapshot {
        total_bytes: total,
        used_bytes: used.min(total),
        available_bytes: available.min(total),
        used_percent: percent(used, total),
        cached_bytes: extras.cached_bytes,
        buffers_bytes: extras.buffers_bytes,
        swap_total_bytes: swap_total,
        swap_used_bytes: swap_used.min(swap_total),
        commit_used_bytes: extras.commit_used_bytes,
        commit_limit_bytes: extras.commit_limit_bytes,
        kernel_paged_bytes: extras.kernel_paged_bytes,
        kernel_nonpaged_bytes: extras.kernel_nonpaged_bytes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_percent_and_clamps() {
        let m = build(1000, 250, 750, 100, 500, MemoryExtras::default());
        assert_eq!(m.used_percent, 25.0);
        assert_eq!(m.swap_used_bytes, 100);
        assert_eq!(m.cached_bytes, None);
    }

    #[test]
    fn zero_total_does_not_divide_by_zero() {
        let m = build(0, 0, 0, 0, 0, MemoryExtras::default());
        assert_eq!(m.used_percent, 0.0);
    }

    #[test]
    fn preserves_pool_figures_without_treating_them_as_extra_ram_usage() {
        let m = build(
            1000,
            920,
            80,
            500,
            100,
            MemoryExtras {
                kernel_paged_bytes: Some(200),
                kernel_nonpaged_bytes: Some(100),
                ..MemoryExtras::default()
            },
        );
        assert_eq!(m.used_bytes, 920);
        assert_eq!(m.kernel_paged_bytes, Some(200));
        assert_eq!(m.kernel_nonpaged_bytes, Some(100));
    }
}
