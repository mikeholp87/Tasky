//! External volumes, mount points and safe device removal using Win32 APIs.
use std::fs::{File, OpenOptions};
use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle};

use windows::Win32::Devices::DeviceAndDriverInstallation::*;
use windows::Win32::Foundation::{E_ACCESSDENIED, ERROR_NO_MORE_FILES, HANDLE};
use windows::Win32::Storage::FileSystem::*;
use windows::Win32::System::IO::DeviceIoControl;
use windows::Win32::System::Ioctl::*;
use windows::core::{GUID, PCWSTR};

use crate::errors::{AppError, AppResult};
use crate::models::Volume;

static MANAGEMENT: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn text(s: &[u16]) -> String {
    String::from_utf16_lossy(&s[..s.iter().position(|&c| c == 0).unwrap_or(s.len())])
}
fn error(e: windows::core::Error) -> AppError {
    if e.code() == E_ACCESSDENIED {
        AppError::PermissionDenied("Windows denied access to this drive. Mounting or unmounting may require administrator rights.".into())
    } else {
        AppError::SystemQueryFailed(format!("Windows drive operation: {}", e.message()))
    }
}
fn open(path: &str, access: u32) -> AppResult<File> {
    OpenOptions::new()
        .read(true)
        .access_mode(access)
        .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0)
        .open(path.trim_end_matches('\\'))
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                AppError::PermissionDenied(
                    "Windows denied access to this drive. Administrator rights may be required."
                        .into(),
                )
            } else {
                AppError::SystemQueryFailed(format!("Cannot open this drive: {e}"))
            }
        })
}
fn handle(file: &File) -> HANDLE {
    HANDLE(file.as_raw_handle())
}

fn device_number(file: &File) -> AppResult<u32> {
    let mut number = STORAGE_DEVICE_NUMBER::default();
    let mut returned = 0;
    // SAFETY: valid live file handle and appropriately sized output structure.
    unsafe {
        DeviceIoControl(
            handle(file),
            IOCTL_STORAGE_GET_DEVICE_NUMBER,
            None,
            0,
            Some((&mut number as *mut STORAGE_DEVICE_NUMBER).cast()),
            std::mem::size_of_val(&number) as u32,
            Some(&mut returned),
            None,
        )
    }
    .map_err(error)?;
    if returned < std::mem::size_of_val(&number) as u32 {
        return Err(AppError::SystemQueryFailed(
            "Incomplete storage-device identity.".into(),
        ));
    }
    Ok(number.DeviceNumber)
}

fn external(file: &File) -> bool {
    let query = STORAGE_PROPERTY_QUERY {
        PropertyId: StorageDeviceProperty,
        QueryType: PropertyStandardQuery,
        ..Default::default()
    };
    let mut buffer = vec![0usize; 1024];
    let mut returned = 0;
    // SAFETY: query and aligned descriptor buffer are valid for the call. A
    // failed or truncated response never grants drive-management eligibility.
    if unsafe {
        DeviceIoControl(
            handle(file),
            IOCTL_STORAGE_QUERY_PROPERTY,
            Some((&query as *const STORAGE_PROPERTY_QUERY).cast()),
            std::mem::size_of_val(&query) as u32,
            Some(buffer.as_mut_ptr().cast()),
            std::mem::size_of_val(buffer.as_slice()) as u32,
            Some(&mut returned),
            None,
        )
    }
    .is_err()
        || (returned as usize) < std::mem::size_of::<STORAGE_DEVICE_DESCRIPTOR>()
    {
        return false;
    }
    let descriptor = unsafe { &*buffer.as_ptr().cast::<STORAGE_DEVICE_DESCRIPTOR>() };
    // USB external SSDs often report themselves as fixed media. Internal SATA,
    // NVMe, virtual and network storage are never accepted.
    descriptor.BusType == BusTypeUsb
        || (descriptor.RemovableMedia
            && (descriptor.BusType == BusTypeSd || descriptor.BusType == BusTypeMmc))
}

fn mount_paths(device: &str) -> AppResult<Vec<String>> {
    let device = wide(device);
    let mut buffer = vec![0u16; 32768];
    let mut needed = 0;
    // SAFETY: valid terminated volume GUID and bounded writable MULTI_SZ.
    unsafe {
        GetVolumePathNamesForVolumeNameW(PCWSTR(device.as_ptr()), Some(&mut buffer), &mut needed)
    }
    .map_err(error)?;
    Ok(buffer
        .split(|&c| c == 0)
        .take_while(|s| !s.is_empty())
        .map(String::from_utf16_lossy)
        .collect())
}

struct VolumeSearch(HANDLE);
impl Drop for VolumeSearch {
    fn drop(&mut self) {
        // SAFETY: handle was created by FindFirstVolumeW, closed once.
        let _ = unsafe { FindVolumeClose(self.0) };
    }
}

struct Entry {
    volume: Volume,
    disk: u32,
    paths: Vec<String>,
}

fn system_disk() -> AppResult<u32> {
    let root = super::control::system_root().ok_or_else(|| {
        AppError::ActionUnavailable("Cannot identify the Windows system drive.".into())
    })?;
    let root = wide(&root);
    let mut path = [0u16; 1024];
    let mut volume = [0u16; 1024];
    // SAFETY: paths are terminated and both output buffers are bounded.
    unsafe { GetVolumePathNameW(PCWSTR(root.as_ptr()), &mut path) }.map_err(error)?;
    unsafe { GetVolumeNameForVolumeMountPointW(PCWSTR(path.as_ptr()), &mut volume) }
        .map_err(error)?;
    device_number(&open(&text(&volume), 0)?)
}

fn entries() -> AppResult<Vec<Entry>> {
    // Refuse every partition on the OS disk, including external Windows
    // installations, instead of protecting only the Windows drive letter.
    let system_disk = system_disk()?;
    let mut name = [0u16; 1024];
    // SAFETY: valid writable volume-name buffer.
    let search = VolumeSearch(unsafe { FindFirstVolumeW(&mut name) }.map_err(error)?);
    let mut entries = Vec::new();
    loop {
        let device = text(&name);
        if let Ok(file) = open(&device, 0)
            && external(&file)
        {
            let disk = device_number(&file)?;
            let paths = mount_paths(&device)?;
            // External Windows installations and folder-mounted volumes
            // remain read-only here. Only single-letter roots are managed.
            let system_root = super::control::system_root()
                .unwrap_or_default()
                .to_ascii_lowercase();
            let safe = disk != system_disk
                && paths
                    .iter()
                    .all(|p| drive_root(p) && !system_root.starts_with(&p.to_ascii_lowercase()));
            let mut label = [0u16; 256];
            let mut fs = [0u16; 256];
            let device_wide = wide(&device);
            // SAFETY: volume name and output buffers are valid.
            let readable = unsafe {
                GetVolumeInformationW(
                    PCWSTR(device_wide.as_ptr()),
                    Some(&mut label),
                    None,
                    None,
                    None,
                    Some(&mut fs),
                )
            }
            .is_ok();
            let mut total = 0;
            let _ = unsafe {
                GetDiskFreeSpaceExW(PCWSTR(device_wide.as_ptr()), None, Some(&mut total), None)
            };
            if safe && readable {
                let label = text(&label);
                entries.push(Entry {
                    disk,
                    paths: paths.clone(),
                    volume: Volume {
                        device,
                        label: (!label.is_empty()).then_some(label),
                        file_system: text(&fs),
                        size_bytes: total,
                        mount_point: paths.first().cloned(),
                        can_power_off: true,
                    },
                });
            }
        }
        // SAFETY: valid enumeration handle and writable output.
        match unsafe { FindNextVolumeW(search.0, &mut name) } {
            Ok(()) => {}
            Err(e) if e.code() == ERROR_NO_MORE_FILES.to_hresult() => break,
            Err(e) => return Err(error(e)),
        }
    }
    Ok(entries)
}

fn drive_root(path: &str) -> bool {
    let p = path.as_bytes();
    p.len() == 3 && p[0].is_ascii_alphabetic() && p[1] == b':' && p[2] == b'\\'
}
fn validate(device: &str) -> AppResult<()> {
    let guid = device
        .strip_prefix("\\\\?\\Volume{")
        .and_then(|s| s.strip_suffix("}\\"));
    if !guid.is_some_and(|s| s.len() == 36 && s.chars().all(|c| c.is_ascii_hexdigit() || c == '-'))
    {
        return Err(AppError::InvalidArgument(
            "Select a volume from the current external-drive list.".into(),
        ));
    }
    Ok(())
}
fn selected(device: &str) -> AppResult<Entry> {
    validate(device)?;
    entries()?.into_iter().find(|e| e.volume.device.eq_ignore_ascii_case(device))
        .ok_or_else(|| AppError::ActionUnavailable("This drive is no longer available or is not eligible for external-drive management.".into()))
}

pub fn volumes() -> AppResult<Vec<Volume>> {
    Ok(entries()?.into_iter().map(|e| e.volume).collect())
}

pub fn mount_volume(device: &str) -> AppResult<Option<String>> {
    let _guard = MANAGEMENT.try_lock().map_err(|_| AppError::OperationBusy)?;
    let entry = selected(device)?;
    if !entry.paths.is_empty() {
        return Ok(entry.volume.mount_point);
    }
    // SAFETY: GetLogicalDrives has no pointer arguments.
    let used = unsafe { GetLogicalDrives() };
    if used == 0 {
        return Err(error(windows::core::Error::from_thread()));
    }
    let letter = (3u8..26)
        .find(|i| used & (1u32 << i) == 0)
        .ok_or_else(|| AppError::ActionUnavailable("No free drive letter is available.".into()))?;
    let path = format!("{}:\\", char::from(b'A' + letter));
    let path_wide = wide(&path);
    let device = wide(&entry.volume.device);
    // SAFETY: both strings are terminated; Windows refuses occupied letters.
    unsafe { SetVolumeMountPointW(PCWSTR(path_wide.as_ptr()), PCWSTR(device.as_ptr())) }
        .map_err(error)?;
    Ok(Some(path))
}

pub fn unmount_volume(device: &str) -> AppResult<()> {
    let _guard = MANAGEMENT.try_lock().map_err(|_| AppError::OperationBusy)?;
    let entry = selected(device)?;
    if entry.paths.is_empty() {
        return Ok(());
    }
    if entry.paths.len() != 1 {
        return Err(AppError::ActionUnavailable(
            "This volume has multiple mount points. Manage it in Windows Disk Management.".into(),
        ));
    }
    let file = open(
        &entry.volume.device,
        windows::Win32::Foundation::GENERIC_READ.0 | windows::Win32::Foundation::GENERIC_WRITE.0,
    )?;
    // SAFETY: exclusive lock is required before dismounting. Windows refuses
    // locks on system/pagefile volumes or volumes with open files. No force.
    unsafe { DeviceIoControl(handle(&file), FSCTL_LOCK_VOLUME, None, 0, None, 0, None, None) }
        .map_err(|_| AppError::ActionUnavailable("Windows could not lock this drive. Close files and programs using it, and check administrator permissions.".into()))?;
    unsafe {
        DeviceIoControl(
            handle(&file),
            FSCTL_DISMOUNT_VOLUME,
            None,
            0,
            None,
            0,
            None,
            None,
        )
    }
    .map_err(error)?;
    let path = wide(&entry.paths[0]);
    // SAFETY: the mount point came from the native list and the volume is locked.
    unsafe { DeleteVolumeMountPointW(PCWSTR(path.as_ptr())) }.map_err(error)?;
    Ok(())
}

struct DeviceSet(HDEVINFO);
impl Drop for DeviceSet {
    fn drop(&mut self) {
        // SAFETY: owned SetupAPI device list, closed exactly once.
        let _ = unsafe { SetupDiDestroyDeviceInfoList(self.0) };
    }
}
fn disk_node(disk: u32) -> AppResult<u32> {
    let class: GUID = GUID_DEVINTERFACE_DISK;
    // SAFETY: enumerate only present disk interfaces on the local machine.
    let set = DeviceSet(
        unsafe {
            SetupDiGetClassDevsW(
                Some(&class),
                PCWSTR::null(),
                None,
                DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
            )
        }
        .map_err(error)?,
    );
    for index in 0..1024 {
        let mut interface = SP_DEVICE_INTERFACE_DATA {
            cbSize: std::mem::size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
            ..Default::default()
        };
        // SAFETY: valid list, interface GUID and initialized output struct.
        if unsafe { SetupDiEnumDeviceInterfaces(set.0, None, &class, index, &mut interface) }
            .is_err()
        {
            break;
        }
        let mut buffer = vec![0usize; 4096];
        let detail = buffer
            .as_mut_ptr()
            .cast::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>();
        let mut info = SP_DEVINFO_DATA {
            cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
            ..Default::default()
        };
        // SAFETY: aligned buffer, cbSize matches target architecture; SetupAPI
        // writes a terminated DevicePath within this allocation on success.
        unsafe {
            (*detail).cbSize = std::mem::size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
        }
        if unsafe {
            SetupDiGetDeviceInterfaceDetailW(
                set.0,
                &interface,
                Some(detail),
                std::mem::size_of_val(buffer.as_slice()) as u32,
                None,
                Some(&mut info),
            )
        }
        .is_ok()
        {
            let path =
                unsafe { PCWSTR((*detail).DevicePath.as_ptr()).to_string() }.unwrap_or_default();
            if let Ok(file) = open(&path, 0)
                && device_number(&file).ok() == Some(disk)
            {
                return Ok(info.DevInst);
            }
        }
    }
    Err(AppError::ActionUnavailable(
        "Windows could not identify this disk for safe removal.".into(),
    ))
}

/// Check *every* volume on this physical disk, including volumes omitted from
/// the manageable list. A mounted sibling prevents removal of the whole disk.
fn all_unmounted(disk: u32) -> AppResult<()> {
    let mut name = [0u16; 1024];
    let search = VolumeSearch(unsafe { FindFirstVolumeW(&mut name) }.map_err(error)?);
    loop {
        let device = text(&name);
        if let Ok(file) = open(&device, 0)
            && device_number(&file).ok() == Some(disk)
            && !mount_paths(&device)?.is_empty()
        {
            return Err(AppError::ActionUnavailable(
                "Unmount every volume on this disk before ejecting it.".into(),
            ));
        }
        match unsafe { FindNextVolumeW(search.0, &mut name) } {
            Ok(()) => {}
            Err(e) if e.code() == ERROR_NO_MORE_FILES.to_hresult() => return Ok(()),
            Err(e) => return Err(error(e)),
        }
    }
}

pub fn power_off_volume(device: &str) -> AppResult<()> {
    let _guard = MANAGEMENT.try_lock().map_err(|_| AppError::OperationBusy)?;
    let entry = selected(device)?;
    all_unmounted(entry.disk)?;
    let node = disk_node(entry.disk)?;
    let mut veto = PNP_VETO_TYPE::default();
    let mut reason = [0u16; 1024];
    // SAFETY: present disk devnode resolved by physical device number. PnP
    // performs a safe eject and vetoes busy devices; no forced removal flags.
    let result = unsafe { CM_Request_Device_EjectW(node, Some(&mut veto), Some(&mut reason), 0) };
    if result != CR_SUCCESS {
        return Err(AppError::ActionUnavailable(format!(
            "Windows refused safe removal ({}). Close programs using the disk and try again. {}",
            result.0,
            text(&reason)
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_ids_and_limits_mount_points() {
        assert!(validate("C:\\").is_err());
        assert!(validate("\\\\.\\PhysicalDrive0").is_err());
        assert!(validate("\\\\?\\Volume{12345678-1234-1234-1234-123456789abc}\\").is_ok());
        assert!(drive_root("E:\\"));
        assert!(!drive_root("C:\\Windows\\"));
    }
    #[test]
    fn native_enumeration_never_offers_the_windows_volume() {
        let system = super::super::control::system_root()
            .unwrap()
            .to_ascii_lowercase();
        for v in volumes().unwrap() {
            if let Some(path) = v.mount_point {
                assert!(!system.starts_with(&path.to_ascii_lowercase()));
            }
        }
    }
}
