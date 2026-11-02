use ::windows::Win32::System::ProcessStatus::{GetPerformanceInfo, PERFORMANCE_INFORMATION};

use crate::platform::MemoryExtras;

/// System cache and commit charge via `GetPerformanceInfo`.
/// <https://learn.microsoft.com/windows/win32/api/psapi/nf-psapi-getperformanceinfo>
pub fn memory_extras() -> MemoryExtras {
    let mut info = PERFORMANCE_INFORMATION {
        cb: size_of::<PERFORMANCE_INFORMATION>() as u32,
        ..Default::default()
    };
    // SAFETY: `info` is a valid, correctly sized out-parameter.
    if unsafe { GetPerformanceInfo(&mut info, info.cb) }.is_err() {
        tracing::debug!("GetPerformanceInfo failed");
        return MemoryExtras::default();
    }
    // Values are in pages.
    let page = info.PageSize as u64;
    let bytes = |pages: usize| (pages as u64).checked_mul(page);
    MemoryExtras {
        cached_bytes: bytes(info.SystemCache),
        buffers_bytes: None,
        commit_used_bytes: bytes(info.CommitTotal),
        commit_limit_bytes: bytes(info.CommitLimit),
    }
}
