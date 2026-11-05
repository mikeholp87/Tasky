use std::collections::HashMap;

use ::windows::Wdk::System::SystemInformation::{
    NtQuerySystemInformation, SystemProcessInformation,
};
use ::windows::Win32::Foundation::{NTSTATUS, STATUS_INFO_LENGTH_MISMATCH};
use ::windows::Win32::System::WindowsProgramming::SYSTEM_PROCESS_INFORMATION;

use crate::platform::ProcessExtras;

/// Per-process figures for *every* process from one system-wide snapshot.
///
/// Unlike per-process queries this needs no process handle, so it also covers
/// protected/system processes an unelevated app cannot open. Only the
/// documented fields of `SYSTEM_PROCESS_INFORMATION` are read.
/// <https://learn.microsoft.com/windows/win32/api/winternl/nf-winternl-ntquerysysteminformation>
pub fn process_extras(_pids: &[u32]) -> HashMap<u32, ProcessExtras> {
    let Some(buf) = query_snapshot() else {
        return HashMap::new();
    };
    let mut out = HashMap::with_capacity(512);
    let base = buf.as_ptr().cast::<u8>();
    let len = buf.len() * size_of::<u64>();
    let mut offset = 0usize;
    loop {
        if offset
            .checked_add(size_of::<SYSTEM_PROCESS_INFORMATION>())
            .is_none_or(|end| end > len)
        {
            break;
        }
        // SAFETY: the complete structure is in bounds. Read by value so
        // a malformed or changed offset cannot create an unaligned reference.
        let entry = unsafe {
            std::ptr::read_unaligned(base.add(offset).cast::<SYSTEM_PROCESS_INFORMATION>())
        };
        let pid = entry.UniqueProcessId.0 as usize as u32;
        out.insert(
            pid,
            ProcessExtras {
                threads: Some(entry.NumberOfThreads),
                priority: Some(entry.BasePriority),
                memory_bytes: Some(entry.WorkingSetSize as u64),
                virtual_memory_bytes: Some(entry.VirtualSize as u64),
            },
        );
        if entry.NextEntryOffset == 0 {
            break;
        }
        match offset.checked_add(entry.NextEntryOffset as usize) {
            Some(next) if next > offset => offset = next,
            _ => break,
        }
    }
    out
}

/// Fills a buffer with the process snapshot, growing it while the process
/// list changes size between calls.
fn query_snapshot() -> Option<Vec<u64>> {
    let mut bytes: u32 = 512 * 1024;
    for _ in 0..5 {
        if bytes > 16 * 1024 * 1024 {
            return None;
        }
        let mut buf = vec![0u64; (bytes as usize).div_ceil(size_of::<u64>())];
        let mut needed = 0u32;
        // SAFETY: the buffer is `bytes` long and writable.
        let status: NTSTATUS = unsafe {
            NtQuerySystemInformation(
                SystemProcessInformation,
                buf.as_mut_ptr().cast(),
                bytes,
                &mut needed,
            )
        };
        if status == STATUS_INFO_LENGTH_MISMATCH {
            // Leave headroom for processes started in the meantime.
            bytes = needed.max(bytes).saturating_add(64 * 1024);
            continue;
        }
        if status.is_err() {
            tracing::debug!("NtQuerySystemInformation failed: {status:?}");
            return None;
        }
        return Some(buf);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_includes_this_process_and_system() {
        let extras = process_extras(&[]);
        let me = extras
            .get(&std::process::id())
            .expect("own process present");
        assert!(me.memory_bytes.unwrap_or(0) > 0);
        assert!(me.threads.unwrap_or(0) >= 1);
        // PID 4 ("System") is protected but still reported.
        assert!(extras.get(&4).and_then(|e| e.memory_bytes).is_some());
    }
}
