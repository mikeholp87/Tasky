//! What the kernel says about the physical device behind a filesystem. Reads
//! only world-readable sysfs files, so no privileges are needed, and every
//! field is optional: a value the kernel does not give is not guessed.

use super::super::model::DiskDeviceInfo;

/// "nvme", "usb", "sata" or "mmc" from a canonical sysfs device path.
pub fn transport_from_path(sysfs_path: &str) -> Option<&'static str> {
    // Order matters: a USB drive's path also contains "ata"-like segments.
    if sysfs_path.contains("/usb") {
        Some("usb")
    } else if sysfs_path.contains("/nvme/") || sysfs_path.contains("/nvme") {
        Some("nvme")
    } else if sysfs_path.contains("/mmc") {
        Some("mmc")
    } else if sysfs_path.contains("/ata") {
        Some("sata")
    } else {
        None
    }
}

/// `queue/rotational`: "1" is a spinning disk, "0" is not.
pub fn parse_rotational(text: &str) -> Option<bool> {
    match text.trim() {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}

#[cfg(target_os = "linux")]
pub fn device_info(device_path: &str) -> Option<DiskDeviceInfo> {
    use std::path::Path;
    let name = Path::new(device_path).file_name()?.to_str()?;
    // Device-mapper (LVM, LUKS): follow the first backing device.
    let name = if name.starts_with("dm-") || device_path.contains("/mapper/") {
        let real = std::fs::canonicalize(device_path).ok()?;
        let dm = real.file_name()?.to_str()?.to_owned();
        std::fs::read_dir(format!("/sys/class/block/{dm}/slaves"))
            .ok()?
            .flatten()
            .next()?
            .file_name()
            .into_string()
            .ok()?
    } else {
        name.to_owned()
    };
    let sys = std::fs::canonicalize(format!("/sys/class/block/{name}")).ok()?;
    // A partition's sysfs directory is inside its disk's directory.
    let disk_dir = if sys.join("partition").exists() {
        sys.parent()?.to_path_buf()
    } else {
        sys.clone()
    };
    let disk = disk_dir.file_name()?.to_str()?.to_owned();
    let read = |p: &str| std::fs::read_to_string(disk_dir.join(p)).ok();
    Some(DiskDeviceInfo {
        model: read("device/model")
            .map(|m| m.trim().to_owned())
            .filter(|m| !m.is_empty()),
        rotational: read("queue/rotational").and_then(|r| parse_rotational(&r)),
        transport: transport_from_path(&disk_dir.to_string_lossy()).map(str::to_owned),
        name: disk,
    })
}

#[cfg(not(target_os = "linux"))]
pub fn device_info(_device_path: &str) -> Option<DiskDeviceInfo> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transports_are_read_from_the_sysfs_path() {
        assert_eq!(
            transport_from_path(
                "/sys/devices/pci0000:00/0000:00:06.0/0000:01:00.0/nvme/nvme0/nvme0n1"
            ),
            Some("nvme")
        );
        assert_eq!(
            transport_from_path(
                "/sys/devices/pci0000:00/0000:00:17.0/ata1/host0/target0:0:0/0:0:0:0/block/sda"
            ),
            Some("sata")
        );
        assert_eq!(
            transport_from_path(
                "/sys/devices/pci0000:00/0000:00:14.0/usb2/2-1/2-1:1.0/host6/target6:0:0/6:0:0:0/block/sdb"
            ),
            Some("usb")
        );
        assert_eq!(
            transport_from_path("/sys/devices/virtual/block/zram0"),
            None
        );
    }

    #[test]
    fn rotational_flag() {
        assert_eq!(parse_rotational("1\n"), Some(true));
        assert_eq!(parse_rotational("0"), Some(false));
        assert_eq!(parse_rotational("maybe"), None);
        assert_eq!(parse_rotational(""), None);
    }

    #[test]
    fn a_nonexistent_device_gives_nothing_rather_than_a_guess() {
        assert_eq!(device_info("/dev/definitely-not-a-device"), None);
    }

    /// Live: this machine's root filesystem device resolves to a disk.
    #[test]
    #[ignore = "reads this machine's sysfs"]
    fn live_device_info_for_the_nvme_drive() {
        let info = device_info("/dev/nvme0n1p2").expect("nvme partition resolves");
        assert_eq!(info.name, "nvme0n1");
        assert_eq!(info.rotational, Some(false));
        assert_eq!(info.transport.as_deref(), Some("nvme"));
    }
}
