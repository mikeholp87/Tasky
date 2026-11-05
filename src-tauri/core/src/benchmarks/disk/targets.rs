//! Where the disk benchmark may create its file. The UI chooses from this
//! list by id and never supplies a path, so a hostile or buggy frontend
//! cannot point the benchmark at an arbitrary location.

use std::path::{Path, PathBuf};

use super::super::model::DiskTargetInfo;
use super::plan::{MIN_FILE_BYTES, MIN_RESERVE};

/// A mounted filesystem, as the monitoring engine reports it.
#[derive(Debug, Clone)]
pub struct MountInfo {
    pub mount_point: String,
    pub file_system: String,
    pub device: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub read_only: bool,
}

/// Filesystems the benchmark never uses: memory-backed (it would measure RAM
/// and eat memory), network (a hung mount can block forever), and virtual.
fn excluded(fs: &str) -> bool {
    let fs = fs.to_ascii_lowercase();
    matches!(
        fs.as_str(),
        "tmpfs"
            | "ramfs"
            | "squashfs"
            | "devtmpfs"
            | "nfs"
            | "nfs4"
            | "cifs"
            | "smb3"
            | "smbfs"
            | "sshfs"
            | "9p"
            | "afs"
            | "ceph"
            | "glusterfs"
            | "iso9660"
            | "udf"
            | "autofs"
    ) || fs.starts_with("fuse")
}

fn note_for(fs: &str) -> Option<String> {
    match fs.to_ascii_lowercase().as_str() {
        "overlay" | "overlay2" => Some(
            "Overlay filesystem (common in containers): results reflect the container's storage layer."
                .into(),
        ),
        "btrfs" | "zfs" => Some(
            "Copy-on-write filesystem: results include its overhead, not just the drive's speed."
                .into(),
        ),
        _ => None,
    }
}

/// Lowercase letters, digits and dashes only, for ids.
pub fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let t = out.trim_matches('-');
    if t.is_empty() {
        "root".into()
    } else {
        t.to_owned()
    }
}

/// The mount containing `path`: the longest mount point that is a prefix.
pub fn mount_for<'a>(path: &Path, mounts: &'a [MountInfo]) -> Option<&'a MountInfo> {
    mounts
        .iter()
        .filter(|m| path.starts_with(&m.mount_point))
        .max_by_key(|m| m.mount_point.len())
}

/// Builds the list of usable locations. `writable` says whether this user can
/// create files in a folder (injected so the rules are testable).
pub fn build_targets(
    mounts: &[MountInfo],
    home: Option<&Path>,
    tmp: &Path,
    writable: &dyn Fn(&Path) -> bool,
) -> Vec<DiskTargetInfo> {
    let usable = |m: &MountInfo| !m.read_only && !excluded(&m.file_system);
    let make = |id: String, label: String, dir: PathBuf, m: &MountInfo| DiskTargetInfo {
        id,
        label,
        directory: dir.to_string_lossy().into_owned(),
        mount_point: m.mount_point.clone(),
        file_system: m.file_system.clone(),
        device: (!m.device.is_empty()).then(|| m.device.clone()),
        free_bytes: m.free_bytes,
        total_bytes: m.total_bytes,
        recommended: false,
        notes: note_for(&m.file_system).into_iter().collect(),
    };

    let mut out = Vec::new();
    let home_mount = home
        .and_then(|h| mount_for(h, mounts))
        .filter(|m| usable(m));
    if let (Some(h), Some(m)) = (home, home_mount)
        && writable(h)
    {
        out.push(make(
            "home".into(),
            format!("Home folder ({})", h.display()),
            h.to_path_buf(),
            m,
        ));
    }
    // Every other usable filesystem the user can write to.
    for m in mounts.iter().filter(|m| usable(m)) {
        let is_home_mount = home_mount.is_some_and(|h| h.mount_point == m.mount_point);
        if is_home_mount || m.free_bytes < MIN_FILE_BYTES + MIN_RESERVE {
            continue;
        }
        let dir = Path::new(&m.mount_point);
        if writable(dir) {
            out.push(make(
                format!("mnt-{}", slug(&m.mount_point)),
                m.mount_point.clone(),
                dir.to_path_buf(),
                m,
            ));
        }
    }
    // The temporary folder, when it is on a real drive of its own.
    if let Some(m) = mount_for(tmp, mounts).filter(|m| usable(m))
        && !out.iter().any(|t| t.mount_point == m.mount_point)
        && writable(tmp)
    {
        out.push(make(
            "tmp".into(),
            format!("Temporary folder ({})", tmp.display()),
            tmp.to_path_buf(),
            m,
        ));
    }
    if let Some(first) = out.first_mut() {
        first.recommended = true;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1 << 30;

    fn mount(point: &str, fs: &str, free_gib: u64) -> MountInfo {
        MountInfo {
            mount_point: point.into(),
            file_system: fs.into(),
            device: format!("/dev/x{}", slug(point)),
            total_bytes: 500 * GIB,
            free_bytes: free_gib * GIB,
            read_only: false,
        }
    }

    fn all(_: &Path) -> bool {
        true
    }

    #[test]
    fn home_is_recommended_and_listed_first() {
        let mounts = [mount("/", "ext4", 100), mount("/home", "ext4", 500)];
        let t = build_targets(
            &mounts,
            Some(Path::new("/home/me")),
            Path::new("/tmp"),
            &all,
        );
        assert_eq!(t[0].id, "home");
        assert!(t[0].recommended);
        assert_eq!(t[0].mount_point, "/home", "the longest matching mount");
        assert_eq!(t[0].directory, "/home/me");
        assert_eq!(t.iter().filter(|x| x.recommended).count(), 1);
        // The root drive is offered too, as its own location.
        assert!(t.iter().any(|x| x.id == "mnt-root" && x.directory == "/"));
    }

    #[test]
    fn memory_network_and_virtual_filesystems_are_never_offered() {
        let mounts = [
            mount("/", "ext4", 100),
            mount("/dev/shm", "tmpfs", 50),
            mount("/mnt/nas", "nfs4", 900),
            mount("/mnt/nas2", "cifs", 900),
            mount("/mnt/fuse", "fuse.sshfs", 900),
            mount("/snap/core", "squashfs", 0),
        ];
        let t = build_targets(
            &mounts,
            Some(Path::new("/home/me")),
            Path::new("/tmp"),
            &all,
        );
        let offered: Vec<&str> = t.iter().map(|x| x.mount_point.as_str()).collect();
        assert_eq!(offered, ["/"]);
    }

    #[test]
    fn read_only_unwritable_and_nearly_full_locations_are_skipped() {
        let mut ro = mount("/mnt/ro", "ext4", 100);
        ro.read_only = true;
        let mounts = [
            mount("/", "ext4", 100),
            ro,
            mount("/mnt/root-owned", "ext4", 100),
            mount("/mnt/full", "ext4", 1),
        ];
        let writable = |p: &Path| p != Path::new("/mnt/root-owned");
        let t = build_targets(
            &mounts,
            Some(Path::new("/home/me")),
            Path::new("/tmp"),
            &writable,
        );
        assert_eq!(
            t.iter().map(|x| x.mount_point.as_str()).collect::<Vec<_>>(),
            ["/"]
        );
    }

    #[test]
    fn an_unwritable_home_falls_back_to_the_next_location() {
        let mounts = [mount("/", "ext4", 100), mount("/data", "ext4", 100)];
        let t = build_targets(
            &mounts,
            Some(Path::new("/home/me")),
            Path::new("/tmp"),
            &|p| p != Path::new("/home/me"),
        );
        assert!(t.iter().all(|x| x.id != "home"));
        assert!(t[0].recommended);
    }

    #[test]
    fn a_temporary_folder_on_its_own_drive_is_offered_but_not_a_duplicate() {
        let mounts = [mount("/", "ext4", 100), mount("/tmp", "ext4", 100)];
        let t = build_targets(
            &mounts,
            Some(Path::new("/home/me")),
            Path::new("/tmp"),
            &all,
        );
        assert!(t.iter().any(|x| x.id == "mnt-tmp" || x.id == "tmp"));
        // On the same drive as home it would only repeat that location.
        let same = [mount("/", "ext4", 100)];
        let t2 = build_targets(&same, Some(Path::new("/home/me")), Path::new("/tmp"), &all);
        assert!(t2.iter().all(|x| x.id != "tmp"));
        // /tmp on tmpfs is not on any eligible mount at all.
        let ram = [mount("/", "ext4", 100), mount("/tmp", "tmpfs", 8)];
        assert!(
            build_targets(&ram, Some(Path::new("/home/me")), Path::new("/tmp"), &all)
                .iter()
                .all(|x| x.id != "tmp" && x.mount_point != "/tmp")
        );
    }

    #[test]
    fn container_and_copy_on_write_filesystems_carry_a_caution() {
        let t = build_targets(
            &[mount("/", "overlay", 100)],
            Some(Path::new("/root")),
            Path::new("/tmp"),
            &all,
        );
        assert!(t[0].notes[0].contains("container"));
        let b = build_targets(
            &[mount("/", "btrfs", 100)],
            Some(Path::new("/home/me")),
            Path::new("/tmp"),
            &all,
        );
        assert!(b[0].notes[0].contains("Copy-on-write"));
        let e = build_targets(
            &[mount("/", "ext4", 100)],
            Some(Path::new("/home/me")),
            Path::new("/tmp"),
            &all,
        );
        assert!(e[0].notes.is_empty());
    }

    #[test]
    fn ids_are_safe_to_pass_through_a_command() {
        for (input, expect) in [
            ("/", "root"),
            ("/home", "home"),
            ("/mnt/My Disk (2)", "mnt-my-disk-2"),
            ("/boot/efi", "boot-efi"),
        ] {
            let s = slug(input);
            assert_eq!(s, expect);
            assert!(s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'));
        }
    }

    #[test]
    fn no_mounts_means_no_targets_not_a_crash() {
        assert!(
            build_targets(&[], Some(Path::new("/home/me")), Path::new("/tmp"), &all).is_empty()
        );
        assert!(build_targets(&[], None, Path::new("/tmp"), &all).is_empty());
    }
}
