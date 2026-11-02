use std::collections::HashMap;

use ::windows::Win32::Foundation::{CloseHandle, HANDLE};
use ::windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use ::windows::Win32::System::IO::DeviceIoControl;
use ::windows::Win32::System::Ioctl::{DISK_PERFORMANCE, IOCTL_DISK_PERFORMANCE};
use ::windows::core::HSTRING;

use crate::platform::DiskIoCounters;

struct VolumeHandle(HANDLE);

impl Drop for VolumeHandle {
    fn drop(&mut self) {
        // SAFETY: handle from CreateFileW, closed once.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// Operation counts and busy time per drive-letter volume via
/// `IOCTL_DISK_PERFORMANCE`. Opening a volume with no access rights is
/// allowed for unelevated users.
/// <https://learn.microsoft.com/windows/win32/api/winioctl/ni-winioctl-ioctl_disk_performance>
pub fn disk_io_counters(disks: &[(String, String)]) -> HashMap<String, DiskIoCounters> {
    disks
        .iter()
        .filter_map(|(_, mount)| Some((mount.clone(), counters_for(mount)?)))
        .collect()
}

fn counters_for(mount: &str) -> Option<DiskIoCounters> {
    // Only drive-letter mounts ("C:\") map to a volume device path.
    let letter = mount.strip_suffix('\\').unwrap_or(mount);
    if letter.len() != 2 || !letter.ends_with(':') {
        return None;
    }
    let path = HSTRING::from(format!(r"\\.\{letter}"));
    // SAFETY: plain FFI; the handle is owned by VolumeHandle.
    let handle = unsafe {
        CreateFileW(
            &path,
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            None,
        )
    }
    .ok()
    .map(VolumeHandle)?;

    let mut perf = DISK_PERFORMANCE::default();
    let mut returned = 0u32;
    // SAFETY: output buffer is a correctly sized DISK_PERFORMANCE.
    unsafe {
        DeviceIoControl(
            handle.0,
            IOCTL_DISK_PERFORMANCE,
            None,
            0,
            Some((&mut perf as *mut DISK_PERFORMANCE).cast()),
            size_of::<DISK_PERFORMANCE>() as u32,
            Some(&mut returned),
            None,
        )
    }
    .ok()?;

    // Times are in 100 ns units; QueryTime is a timestamp, IdleTime cumulative.
    let to_ns = |v: i64| u64::try_from(v).ok().map(|v| v.saturating_mul(100));
    let idle = to_ns(perf.IdleTime)?;
    let query = to_ns(perf.QueryTime)?;
    Some(DiskIoCounters {
        reads: u64::from(perf.ReadCount),
        writes: u64::from(perf.WriteCount),
        idle_ns: Some(idle),
        clock_ns: Some(query),
        busy_ns: None,
    })
}
