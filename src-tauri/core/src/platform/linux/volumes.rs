//! Mounting and unmounting external drives, through UDisks2 (`udisksctl`).
//!
//! Safety rules, enforced here and not in the UI:
//! - Only volumes UDisks itself treats as user-managed are ever listed
//!   (`HintSystem` and `HintIgnore` false, a real file system). System and
//!   internal partitions are therefore never offered and are refused if a
//!   caller names them anyway.
//! - A volume mounted at or below a system path (`/`, `/home`, `/boot`, ...)
//!   is never touched, whatever UDisks says.
//! - Mount and unmount re-read the current list and act only on a device that
//!   is in it, in the right state. The device string is never trusted.
//! - `udisksctl` runs without a shell, so the device cannot inject arguments.
//!   UDisks/polkit apply their own permission policy on top.

use std::process::Command;

use crate::errors::{AppError, AppResult};
use crate::models::Volume;

/// Mount points that belong to the running system. Matches the path itself
/// and anything below it (`/` only matches exactly).
const SYSTEM_MOUNTS: &[&str] = &[
    "/", "/boot", "/efi", "/home", "/usr", "/var", "/etc", "/opt", "/srv", "/root", "/nix",
];

fn is_system_mount(mount_point: &str) -> bool {
    SYSTEM_MOUNTS
        .iter()
        .any(|p| mount_point == *p || (*p != "/" && mount_point.starts_with(&format!("{p}/"))))
}

/// What `udisksctl info -b` says about one block device.
#[derive(Debug, Default, PartialEq, Eq)]
struct Info {
    system: bool,
    ignore: bool,
    usage: String,
    file_system: String,
    label: String,
    size: u64,
    /// UDisks object path of the physical drive; empty or `/` when none.
    drive: String,
    mount_points: Vec<String>,
}

/// Reads the fields we need. The block section comes first and its `Size`
/// is the one wanted; later interfaces repeat the key.
fn parse_info(text: &str) -> Info {
    let mut info = Info::default();
    let mut have_size = false;
    let mut in_mounts = false;
    for line in text.lines() {
        let trimmed = line.trim();
        // Values that span several lines are indented under their key.
        if in_mounts && !trimmed.contains(": ") && !trimmed.ends_with(':') && !trimmed.is_empty() {
            info.mount_points.push(trimmed.to_string());
            continue;
        }
        in_mounts = false;
        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match key {
            "HintSystem" => info.system = value == "true",
            "HintIgnore" => info.ignore = value == "true",
            "IdUsage" => info.usage = value.to_string(),
            "IdType" => info.file_system = value.to_string(),
            "IdLabel" => info.label = value.to_string(),
            "Drive" => info.drive = value.trim_matches('\'').to_string(),
            "Size" if !have_size => {
                info.size = value.parse().unwrap_or(0);
                have_size = true;
            }
            "MountPoints" => {
                in_mounts = true;
                if !value.is_empty() {
                    info.mount_points.push(value.to_string());
                }
            }
            _ => {}
        }
    }
    info
}

/// The one rule deciding whether a volume may be shown, mounted or unmounted.
fn manageable(info: &Info) -> bool {
    !info.system
        && !info.ignore
        && info.usage == "filesystem"
        && !info.file_system.is_empty()
        && info.file_system != "swap"
        && !info.mount_points.iter().any(|m| is_system_mount(m))
}

fn valid_device(device: &str) -> bool {
    device.len() <= 64
        && device.strip_prefix("/dev/").is_some_and(|name| {
            !name.is_empty()
                && !name.starts_with('.')
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        })
}

fn udisksctl(args: &[&str]) -> AppResult<String> {
    let mut command = Command::new("udisksctl");
    command.args(args);
    // Never wait on a password prompt; `info` does not take the option.
    if args.first() != Some(&"info") {
        command.arg("--no-user-interaction");
    }
    super::tool::run_tool(
        &mut command,
        || {
            AppError::ActionUnavailable(
                "Mounting drives needs UDisks. Install the \"udisks2\" package.".into(),
            )
        },
        |first| {
            if first.contains("busy") {
                AppError::ActionUnavailable(
                    "The drive is in use. Close any program using it and try again.".into(),
                )
            } else if first.contains("Not authorized") || first.contains("not allowed") {
                AppError::PermissionDenied(
                    "Your account is not allowed to do this to that drive.".into(),
                )
            } else {
                AppError::SystemQueryFailed(first.to_string())
            }
        },
    )
}

fn candidates() -> AppResult<Vec<String>> {
    let output = Command::new("lsblk")
        .args(["-n", "-r", "-o", "PATH,TYPE"])
        .env("LC_ALL", "C")
        .output()
        .map_err(|e| AppError::SystemQueryFailed(format!("Could not list drives: {e}")))?;
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let (path, kind) = line.split_once(' ')?;
            matches!(kind, "part" | "disk" | "loop").then(|| path.to_string())
        })
        .filter(|path| valid_device(path))
        .collect())
}

/// Whether UDisks says this drive can be powered off.
fn drive_can_power_off(drive: &str) -> bool {
    if drive.is_empty() || drive == "/" {
        return false;
    }
    udisksctl(&["info", "-p", drive])
        .map(|text| {
            text.lines()
                .any(|l| l.trim().replace(' ', "") == "CanPowerOff:true")
        })
        .unwrap_or(false)
}

pub fn volumes() -> AppResult<Vec<Volume>> {
    let mut out = Vec::new();
    let mut power: std::collections::HashMap<String, bool> = std::collections::HashMap::new();
    for device in candidates()? {
        let Ok(text) = udisksctl(&["info", "-b", &device]) else {
            continue;
        };
        let info = parse_info(&text);
        if !manageable(&info) {
            continue;
        }
        let can_power_off = *power
            .entry(info.drive.clone())
            .or_insert_with(|| drive_can_power_off(&info.drive));
        out.push(Volume {
            device,
            label: (!info.label.is_empty()).then_some(info.label),
            file_system: info.file_system,
            size_bytes: info.size,
            mount_point: info.mount_points.into_iter().next(),
            can_power_off,
        });
    }
    out.sort_by(|a, b| a.device.cmp(&b.device));
    Ok(out)
}

/// A device that is in the current list; anything else is refused.
fn find(device: &str) -> AppResult<Volume> {
    if !valid_device(device) {
        return Err(AppError::InvalidArgument("That is not a drive.".into()));
    }
    volumes()?
        .into_iter()
        .find(|v| v.device == device)
        .ok_or_else(|| {
            AppError::ActionUnavailable(
                "That drive can't be managed here. System and internal partitions are never mounted or unmounted from Tasky."
                    .into(),
            )
        })
}

/// Mounts `device` and returns where it was mounted.
pub fn mount_volume(device: &str) -> AppResult<Option<String>> {
    let volume = find(device)?;
    if volume.mount_point.is_some() {
        return Err(AppError::ActionUnavailable(
            "That drive is already mounted.".into(),
        ));
    }
    let out = udisksctl(&["mount", "-b", &volume.device])?;
    // "Mounted /dev/sdb1 at /run/media/user/LABEL."
    Ok(out
        .split_once(" at ")
        .map(|(_, path)| path.trim().trim_end_matches('.').to_string()))
}

/// One block device and where the kernel says it is mounted.
#[derive(Debug, PartialEq, Eq)]
struct BlockRow {
    path: String,
    /// `/dev/<kernel name>`; `parent` refers to this, not to `path`
    /// (a LUKS volume at `/dev/mapper/luks-…` is `dm-0` to its children).
    kernel: String,
    parent: Option<String>,
    mounted: bool,
}

/// Reads `lsblk -J --list -o PATH,KNAME,PKNAME,MOUNTPOINTS`.
fn parse_block_rows(json: &str) -> Vec<BlockRow> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    value["blockdevices"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|d| {
            let path = d["path"].as_str()?.to_string();
            Some(BlockRow {
                kernel: d["kname"]
                    .as_str()
                    .map_or_else(|| path.clone(), |k| format!("/dev/{k}")),
                path,
                parent: d["pkname"].as_str().map(|p| format!("/dev/{p}")),
                mounted: d["mountpoints"]
                    .as_array()
                    .is_some_and(|m| m.iter().any(|p| p.as_str().is_some())),
            })
        })
        .collect()
}

/// The whole-disk device for `device`, and whether anything on that disk is
/// still mounted: the disk itself or anything stacked on it (partitions,
/// LUKS, LVM), system volumes included. `None` when `device` is not listed,
/// so an incomplete listing never reads as "nothing mounted".
fn disk_of(rows: &[BlockRow], device: &str) -> Option<(String, bool)> {
    let row = rows.iter().find(|r| r.path == device)?;
    let disk = row.parent.clone().unwrap_or_else(|| device.to_string());
    // Stacked devices are listed once per parent, so grow the set of kernel
    // names on this disk until it stops changing.
    let mut on_disk = vec![disk.as_str()];
    loop {
        let before = on_disk.len();
        for r in rows {
            let child = r.parent.as_deref().is_some_and(|p| on_disk.contains(&p));
            if child && !on_disk.contains(&r.kernel.as_str()) {
                on_disk.push(&r.kernel);
            }
        }
        if on_disk.len() == before {
            break;
        }
    }
    let busy = rows
        .iter()
        .any(|r| r.mounted && on_disk.contains(&r.kernel.as_str()));
    Some((disk, busy))
}

/// Powers off the drive holding `device`, after all of its volumes are
/// unmounted. The drive vanishes until it is plugged in again.
pub fn power_off_volume(device: &str) -> AppResult<()> {
    let volume = find(device)?;
    if volume.mount_point.is_some() {
        return Err(AppError::ActionUnavailable(
            "Unmount the drive before powering it off.".into(),
        ));
    }
    if !volume.can_power_off {
        return Err(AppError::ActionUnavailable(
            "This drive can't be powered off from here.".into(),
        ));
    }
    let output = Command::new("lsblk")
        .args(["-J", "--list", "-o", "PATH,KNAME,PKNAME,MOUNTPOINTS"])
        .env("LC_ALL", "C")
        .output()
        .map_err(|e| AppError::SystemQueryFailed(format!("Could not list drives: {e}")))?;
    // MOUNTPOINTS needs util-linux 2.37+; without it the drive can't be shown
    // to be idle, so refuse rather than power off a disk that may be in use.
    let rows = output
        .status
        .success()
        .then(|| parse_block_rows(&String::from_utf8_lossy(&output.stdout)));
    let Some((disk, busy)) = rows.and_then(|rows| disk_of(&rows, &volume.device)) else {
        return Err(AppError::ActionUnavailable(
            "Could not confirm that nothing on this drive is in use, so it was left on.".into(),
        ));
    };
    if busy {
        return Err(AppError::ActionUnavailable(
            "Another volume on this drive is still mounted. Unmount it first.".into(),
        ));
    }
    if !valid_device(&disk) {
        return Err(AppError::InvalidArgument("That is not a drive.".into()));
    }
    udisksctl(&["power-off", "-b", &disk]).map(|_| ())
}

pub fn unmount_volume(device: &str) -> AppResult<()> {
    let volume = find(device)?;
    if volume.mount_point.is_none() {
        return Err(AppError::ActionUnavailable(
            "That drive is not mounted.".into(),
        ));
    }
    udisksctl(&["unmount", "-b", &volume.device]).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    const USB: &str = "\
/org/freedesktop/UDisks2/block_devices/sda1:
  org.freedesktop.UDisks2.Block:
    HintIgnore:                 false
    HintSystem:                 false
    IdLabel:                    BACKUP
    IdType:                     ext4
    IdUsage:                    filesystem
    Size:                       2000397795328
  org.freedesktop.UDisks2.Filesystem:
    MountPoints:        /run/media/mikeh/BACKUP
    Size:               2000397795328
";

    fn system_partition() -> String {
        USB.replace(
            "HintSystem:                 false",
            "HintSystem:                 true",
        )
        .replace("/run/media/mikeh/BACKUP", "/")
    }

    #[test]
    fn reads_an_external_drive() {
        let info = parse_info(USB);
        assert_eq!(info.file_system, "ext4");
        assert_eq!(info.label, "BACKUP");
        assert_eq!(info.size, 2_000_397_795_328);
        assert_eq!(info.mount_points, ["/run/media/mikeh/BACKUP"]);
        assert!(manageable(&info));
    }

    #[test]
    fn never_offers_system_partitions() {
        // UDisks marks the internal disk as a system device.
        assert!(!manageable(&parse_info(&system_partition())));
        // Even if it did not, a system mount point is enough to refuse.
        let unmarked = system_partition().replace(
            "HintSystem:                 true",
            "HintSystem:                 false",
        );
        assert!(!manageable(&parse_info(&unmarked)));
        for mount in [
            "/",
            "/boot/efi",
            "/home",
            "/home/mikeh/data",
            "/var/lib/x",
            "/usr",
        ] {
            assert!(is_system_mount(mount), "{mount}");
        }
        assert!(!is_system_mount("/run/media/mikeh/BACKUP"));
        assert!(!is_system_mount("/mnt/usb"));
    }

    #[test]
    fn skips_things_that_are_not_plain_file_systems() {
        for (from, to) in [
            (
                "IdUsage:                    filesystem",
                "IdUsage:                    crypto",
            ),
            (
                "IdType:                     ext4",
                "IdType:                     swap",
            ),
            (
                "HintIgnore:                 false",
                "HintIgnore:                 true",
            ),
        ] {
            assert!(!manageable(&parse_info(&USB.replace(from, to))), "{to}");
        }
    }

    #[test]
    fn an_unmounted_drive_has_no_mount_point() {
        let text = USB.replace(
            "MountPoints:        /run/media/mikeh/BACKUP",
            "MountPoints:",
        );
        let info = parse_info(&text);
        assert!(info.mount_points.is_empty());
        assert!(manageable(&info));
    }

    #[test]
    fn reads_every_mount_point_of_a_multiply_mounted_drive() {
        let text = USB.replace(
            "MountPoints:        /run/media/mikeh/BACKUP",
            "MountPoints:        /run/media/mikeh/BACKUP\n                        /home/mikeh/backup",
        );
        let info = parse_info(&text);
        assert_eq!(info.mount_points.len(), 2);
        assert!(!manageable(&info), "a mount under /home is a system path");
    }

    #[test]
    fn power_off_waits_for_every_volume_on_the_disk() {
        let rows = parse_block_rows(
            r#"{"blockdevices":[
              {"path":"/dev/sdb","pkname":null,"mountpoints":[null]},
              {"path":"/dev/sdb1","pkname":"sdb","mountpoints":[null]},
              {"path":"/dev/sdb2","pkname":"sdb","mountpoints":["/run/media/a/DATA"]},
              {"path":"/dev/sdc","pkname":null,"mountpoints":[null]},
              {"path":"/dev/sdc1","pkname":"sdc","mountpoints":[null]}
            ]}"#,
        );
        // A sibling partition is still mounted, so sdb1 cannot be powered off yet.
        assert_eq!(
            disk_of(&rows, "/dev/sdb1"),
            Some(("/dev/sdb".to_string(), true))
        );
        // sdc has nothing mounted.
        assert_eq!(
            disk_of(&rows, "/dev/sdc1"),
            Some(("/dev/sdc".to_string(), false))
        );
        // A volume that is the whole disk is its own drive.
        assert_eq!(
            disk_of(&rows, "/dev/sdb"),
            Some(("/dev/sdb".to_string(), true))
        );
    }

    #[test]
    fn power_off_sees_volumes_stacked_on_a_partition() {
        let rows = parse_block_rows(
            r#"{"blockdevices":[
              {"path":"/dev/sdb","pkname":null,"mountpoints":[null]},
              {"path":"/dev/sdb1","pkname":"sdb","mountpoints":[null]},
              {"path":"/dev/sdb2","kname":"sdb2","pkname":"sdb","mountpoints":[null]},
              {"path":"/dev/mapper/luks-1","kname":"dm-0","pkname":"sdb2","mountpoints":[null]},
              {"path":"/dev/mapper/vg-data","kname":"dm-1","pkname":"dm-0","mountpoints":["/data"]}
            ]}"#,
        );
        assert_eq!(
            disk_of(&rows, "/dev/sdb1"),
            Some(("/dev/sdb".to_string(), true))
        );
    }

    #[test]
    fn power_off_refuses_when_the_device_is_not_listed() {
        // An lsblk without MOUNTPOINTS support fails and yields no rows.
        assert_eq!(disk_of(&parse_block_rows(""), "/dev/sdb1"), None);
        let rows = parse_block_rows(
            r#"{"blockdevices":[{"path":"/dev/sdc","pkname":null,"mountpoints":[null]}]}"#,
        );
        assert_eq!(disk_of(&rows, "/dev/sdb1"), None);
    }

    #[test]
    fn reads_the_drive_object_path() {
        let text = USB.replace(
            "HintIgnore:",
            "Drive:                      '/org/freedesktop/UDisks2/drives/Seagate_X'\n    HintIgnore:",
        );
        assert_eq!(
            parse_info(&text).drive,
            "/org/freedesktop/UDisks2/drives/Seagate_X"
        );
    }

    #[test]
    fn only_plain_device_names_are_accepted() {
        for ok in [
            "/dev/sda1",
            "/dev/nvme0n1p2",
            "/dev/mmcblk0p1",
            "/dev/loop0",
        ] {
            assert!(valid_device(ok), "{ok}");
        }
        for bad in [
            "sda1",
            "/dev/",
            "/dev/../etc/passwd",
            "/dev/sda1; rm -rf /",
            "/dev/mapper/root",
            "/dev/sda1 --foo",
            "-b",
            "/dev/.hidden",
        ] {
            assert!(!valid_device(bad), "{bad}");
        }
    }

    #[test]
    fn refuses_unknown_and_malformed_devices_without_running_anything() {
        assert!(matches!(
            mount_volume("/dev/../etc"),
            Err(AppError::InvalidArgument(_))
        ));
        assert!(matches!(
            unmount_volume("not a device"),
            Err(AppError::InvalidArgument(_))
        ));
    }

    /// Needs a desktop session with UDisks: `cargo test -- --ignored lists_real`.
    #[test]
    #[ignore]
    fn lists_real_volumes_and_never_the_system_disk() {
        let all = volumes().unwrap();
        println!("{all:#?}");
        for v in all {
            assert!(
                v.mount_point.as_deref().is_none_or(|m| !is_system_mount(m)),
                "{v:?}"
            );
        }
    }
}
