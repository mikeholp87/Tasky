use std::collections::HashMap;

use ::windows::Win32::Foundation::{CloseHandle, HANDLE};
use ::windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};

use crate::platform::ProcessExtras;

/// Closes the snapshot handle on every exit path.
struct SnapshotHandle(HANDLE);

impl Drop for SnapshotHandle {
    fn drop(&mut self) {
        // SAFETY: the handle came from CreateToolhelp32Snapshot and is closed once.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// Thread counts and base priorities for every process, from one Toolhelp32
/// snapshot. This is a single cheap kernel call regardless of process count.
/// <https://learn.microsoft.com/windows/win32/api/tlhelp32/nf-tlhelp32-createtoolhelp32snapshot>
pub fn process_extras(_pids: &[u32]) -> HashMap<u32, ProcessExtras> {
    let mut out = HashMap::new();
    // SAFETY: plain FFI call; the returned handle is owned by SnapshotHandle.
    let snapshot = match unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) } {
        Ok(h) => SnapshotHandle(h),
        Err(e) => {
            tracing::debug!("CreateToolhelp32Snapshot failed: {e}");
            return out;
        }
    };
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    // SAFETY: `entry` is a correctly sized out-parameter for this snapshot.
    let mut ok = unsafe { Process32FirstW(snapshot.0, &mut entry) }.is_ok();
    while ok {
        out.insert(
            entry.th32ProcessID,
            ProcessExtras {
                threads: Some(entry.cntThreads),
                priority: Some(entry.pcPriClassBase),
            },
        );
        // SAFETY: as above.
        ok = unsafe { Process32NextW(snapshot.0, &mut entry) }.is_ok();
    }
    out
}
