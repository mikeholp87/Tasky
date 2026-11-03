//! Where the storage benchmark may write, and how much.
//!
//! * Only mounted, writable volumes from the monitoring engine's own disk
//!   list can be selected; the frontend sends a mount point, never a path,
//!   and it must match that list exactly.
//! * Files live in an app-owned folder: the app's cache folder on its own
//!   volume, `<mount>/.taskmanager-benchmark/` elsewhere. The folder is
//!   canonicalized and must not be a symlink or resolve outside the volume.
//! * A free-space reserve of max(10 % of capacity, 5 GiB) is always kept;
//!   the test file shrinks to fit, or the test is refused.

use std::path::{Path, PathBuf};

use crate::benchmarks::models::{BenchmarkProfile, DiskLocation, DiskWriteLimit};
use crate::errors::{AppError, AppResult};
use crate::models::DiskSnapshot;

pub const BENCH_DIR_NAME: &str = ".taskmanager-benchmark";
const MIB: u64 = 1024 * 1024;
const GIB: u64 = 1024 * MIB;
/// Smallest file that still produces meaningful sequential results.
pub const MIN_FILE_BYTES: u64 = 256 * MIB;
pub const RESERVE_MIN_BYTES: u64 = 5 * GIB;
/// File sizes are multiples of this (keeps every block aligned).
const SIZE_STEP: u64 = 64 * MIB;

pub const fn target_file_bytes(profile: BenchmarkProfile) -> u64 {
    match profile {
        BenchmarkProfile::Quick => 512 * MIB,
        BenchmarkProfile::Standard => GIB,
        BenchmarkProfile::Extended => 4 * GIB,
    }
}

/// Upper bound of bytes the random-write tests may write.
pub const fn random_write_budget(profile: BenchmarkProfile) -> u64 {
    match profile {
        BenchmarkProfile::Quick => 256 * MIB,
        BenchmarkProfile::Standard => 512 * MIB,
        BenchmarkProfile::Extended => GIB,
    }
}

fn normalize(p: &str) -> String {
    let s = p.replace('\\', "/");
    let s = s.trim_end_matches('/');
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s.to_owned()
    }
}

/// The mount whose path is the longest prefix of `path`.
fn containing_mount<'a>(disks: &'a [DiskSnapshot], path: &Path) -> Option<&'a DiskSnapshot> {
    let target = normalize(&path.to_string_lossy());
    disks
        .iter()
        .filter(|d| {
            let m = normalize(&d.mount_point);
            m.is_empty() || target == m || target.starts_with(&format!("{m}/"))
        })
        .max_by_key(|d| d.mount_point.len())
}

/// Volumes offered for benchmarking (writable, non-empty).
pub fn locations(disks: &[DiskSnapshot], app_dir: Option<&Path>) -> Vec<DiskLocation> {
    let default = app_dir
        .and_then(|p| containing_mount(disks, p))
        .map(|d| d.mount_point.clone());
    disks
        .iter()
        .filter(|d| !d.read_only && d.total_bytes > 0)
        .map(|d| DiskLocation {
            mount_point: d.mount_point.clone(),
            name: d.name.clone(),
            file_system: d.file_system.clone(),
            kind: d.kind,
            removable: d.removable,
            total_bytes: d.total_bytes,
            available_bytes: d.available_bytes,
            is_default: default.as_deref() == Some(d.mount_point.as_str()),
        })
        .collect()
}

/// Resolves the requested mount point (or the default) to a location and
/// the benchmark folder on it.
pub fn resolve(
    disks: &[DiskSnapshot],
    requested: Option<&str>,
    app_dir: Option<&Path>,
) -> AppResult<(DiskLocation, PathBuf)> {
    let options = locations(disks, app_dir);
    let location = match requested {
        Some(mount) => options
            .iter()
            .find(|l| l.mount_point == mount)
            .cloned()
            .ok_or_else(|| {
                AppError::InvalidBenchmarkConfiguration(
                    "The selected drive is not available for benchmarking.".into(),
                )
            })?,
        None => options
            .iter()
            .find(|l| l.is_default)
            .or_else(|| options.iter().find(|l| !l.removable))
            .cloned()
            .ok_or_else(|| {
                AppError::UnsupportedPlatform("No writable drive is available.".into())
            })?,
    };
    let dir = match app_dir {
        Some(app) if location.is_default => app.to_path_buf(),
        _ => Path::new(&location.mount_point).join(BENCH_DIR_NAME),
    };
    Ok((location, dir))
}

/// Creates the benchmark folder and verifies it is a real directory inside
/// the selected volume (no symlink tricks). Returns its canonical path.
pub fn prepare_dir(dir: &Path, mount_point: &str) -> AppResult<PathBuf> {
    std::fs::create_dir_all(dir).map_err(|e| match e.kind() {
        std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::ReadOnlyFilesystem => {
            AppError::DiskPermissionDenied(format!(
                "Task Manager cannot write to {mount_point}. Choose another drive."
            ))
        }
        _ => AppError::SystemQueryFailed(format!("cannot create the benchmark folder: {e}")),
    })?;
    let meta = std::fs::symlink_metadata(dir).map_err(|e| {
        AppError::SystemQueryFailed(format!("cannot inspect benchmark folder: {e}"))
    })?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Err(AppError::DiskPermissionDenied(
            "The benchmark folder is not a regular folder; refusing to use it.".into(),
        ));
    }
    let canonical = std::fs::canonicalize(dir).map_err(|e| {
        AppError::SystemQueryFailed(format!("cannot resolve benchmark folder: {e}"))
    })?;
    let mount = std::fs::canonicalize(mount_point)
        .map_err(|e| AppError::SystemQueryFailed(format!("cannot resolve {mount_point}: {e}")))?;
    if !canonical.starts_with(&mount) {
        return Err(AppError::DiskPermissionDenied(
            "The benchmark folder resolves outside the selected drive; refusing to use it.".into(),
        ));
    }
    Ok(canonical)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskPlan {
    pub file_bytes: u64,
    pub random_write_bytes: u64,
    /// All bytes the run may write (file creation + write tests).
    pub total_write_bytes: u64,
    /// Explanation when the file was made smaller than the profile's size.
    pub reduced: Option<String>,
}

fn gib(bytes: u64) -> String {
    format!("{:.1} GiB", bytes as f64 / GIB as f64)
}

/// Sizes the test within the free-space reserve and the write limit.
pub fn plan(
    profile: BenchmarkProfile,
    allow_write: bool,
    limit: DiskWriteLimit,
    available: u64,
    total: u64,
) -> AppResult<DiskPlan> {
    let target = target_file_bytes(profile);
    let reserve = (total / 10).max(RESERVE_MIN_BYTES);
    // Random writes get at most a quarter of the write limit; the rest is for
    // the test file.
    let random_write = if allow_write {
        random_write_budget(profile).min(limit.max_bytes() / 4)
    } else {
        0
    };
    // Creating the file is itself a write, so it counts against the limit.
    let by_space = available.saturating_sub(reserve);
    let by_limit = limit.max_bytes().saturating_sub(random_write);
    let mut file = target.min(by_space).min(by_limit);
    file -= file % SIZE_STEP;
    if file < MIN_FILE_BYTES {
        return Err(AppError::InsufficientDiskSpace(format!(
            "The storage benchmark needs at least {} of free space beyond a {} safety reserve \
             (available: {}).",
            gib(MIN_FILE_BYTES),
            gib(reserve),
            gib(available)
        )));
    }
    let reduced = (file < target).then(|| {
        if file == by_limit - by_limit % SIZE_STEP && by_limit < by_space {
            format!(
                "Test file reduced to {} to stay within the disk write limit.",
                gib(file)
            )
        } else {
            format!(
                "Test file reduced to {} to keep at least {} free on the drive.",
                gib(file),
                gib(reserve)
            )
        }
    });
    Ok(DiskPlan {
        file_bytes: file,
        random_write_bytes: random_write,
        total_write_bytes: file + random_write,
        reduced,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::DiskKind;

    fn disk(mount: &str, total: u64, available: u64, read_only: bool) -> DiskSnapshot {
        DiskSnapshot {
            name: format!("disk {mount}"),
            mount_point: mount.into(),
            file_system: "ntfs".into(),
            kind: DiskKind::Ssd,
            removable: false,
            read_only,
            total_bytes: total,
            available_bytes: available,
            used_bytes: total - available,
            used_percent: 0.0,
            read_bytes_per_sec: None,
            write_bytes_per_sec: None,
            read_ops_per_sec: None,
            write_ops_per_sec: None,
            active_percent: None,
        }
    }

    #[test]
    fn plans_profile_size_with_plenty_of_space() {
        let p = plan(
            BenchmarkProfile::Standard,
            true,
            DiskWriteLimit::Standard,
            500 * GIB,
            1_000 * GIB,
        )
        .unwrap();
        assert_eq!(p.file_bytes, GIB);
        assert_eq!(p.total_write_bytes, GIB + 512 * MIB);
        assert!(p.reduced.is_none());
    }

    #[test]
    fn keeps_reserve_and_reduces_or_refuses() {
        // 12 GiB free, 100 GiB disk → reserve 10 GiB → ~2 GiB usable.
        let p = plan(
            BenchmarkProfile::Extended,
            false,
            DiskWriteLimit::Extended,
            12 * GIB,
            100 * GIB,
        )
        .unwrap();
        assert_eq!(p.file_bytes, 2 * GIB);
        assert!(p.reduced.unwrap().contains("keep at least"));
        // Too little space beyond the reserve → refused, never filled up.
        let err = plan(
            BenchmarkProfile::Quick,
            true,
            DiskWriteLimit::Standard,
            5 * GIB,
            40 * GIB,
        )
        .unwrap_err();
        assert_eq!(err.kind(), crate::errors::ErrorKind::InsufficientDiskSpace);
    }

    #[test]
    fn write_limit_caps_file_and_random_writes() {
        let p = plan(
            BenchmarkProfile::Extended,
            true,
            DiskWriteLimit::Conservative,
            900 * GIB,
            1_000 * GIB,
        )
        .unwrap();
        assert!(p.total_write_bytes <= DiskWriteLimit::Conservative.max_bytes());
        assert!(p.reduced.unwrap().contains("write limit"));
        // Read-only runs write nothing but the file itself.
        let p = plan(
            BenchmarkProfile::Quick,
            false,
            DiskWriteLimit::Standard,
            900 * GIB,
            1_000 * GIB,
        )
        .unwrap();
        assert_eq!(p.random_write_bytes, 0);
        assert_eq!(p.total_write_bytes, p.file_bytes);
    }

    #[test]
    fn resolves_only_listed_writable_mounts() {
        let disks = vec![
            disk("C:\\", 1_000 * GIB, 500 * GIB, false),
            disk("D:\\", 1_000 * GIB, 500 * GIB, false),
            disk("E:\\", 1_000 * GIB, 500 * GIB, true),
        ];
        let app = Path::new("C:\\Users\\me\\AppData\\Local\\com.taskmanager.app\\benchmark");
        let locs = locations(&disks, Some(app));
        assert_eq!(locs.len(), 2, "read-only volume excluded");
        assert!(locs[0].is_default);

        let (loc, dir) = resolve(&disks, None, Some(app)).unwrap();
        assert_eq!(loc.mount_point, "C:\\");
        assert_eq!(dir, app);
        let (_, dir) = resolve(&disks, Some("D:\\"), Some(app)).unwrap();
        assert_eq!(dir, Path::new("D:\\").join(BENCH_DIR_NAME));
        for bad in [
            "E:\\",
            "C:\\Windows",
            "..\\..",
            "\\\\.\\PhysicalDrive0",
            "/dev/sda",
        ] {
            assert!(resolve(&disks, Some(bad), Some(app)).is_err(), "{bad}");
        }
    }

    #[test]
    fn longest_mount_prefix_wins() {
        let disks = vec![
            disk("/", 100 * GIB, 50 * GIB, false),
            disk("/home", 100 * GIB, 50 * GIB, false),
        ];
        let app = Path::new("/home/me/.cache/com.taskmanager.app/benchmark");
        let locs = locations(&disks, Some(app));
        assert!(
            locs.iter()
                .find(|l| l.mount_point == "/home")
                .unwrap()
                .is_default
        );
        // "/homework" must not match "/home".
        let other = Path::new("/homework/x");
        let m = containing_mount(&disks, other).unwrap();
        assert_eq!(m.mount_point, "/");
    }

    #[test]
    fn prepare_dir_creates_and_canonicalizes_inside_mount() {
        let base = std::env::temp_dir().join(format!("tm-bench-dir-{}", uuid::Uuid::new_v4()));
        let dir = base.join("benchmark");
        let canonical =
            prepare_dir(&dir, &base.to_string_lossy().replace("benchmark", "")).unwrap();
        assert!(canonical.is_dir());
        // A folder outside the claimed mount is refused.
        let elsewhere = std::env::temp_dir().join(format!("tm-bench-mnt-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&elsewhere).unwrap();
        assert!(prepare_dir(&dir, &elsewhere.to_string_lossy()).is_err());
        std::fs::remove_dir_all(base).unwrap();
        std::fs::remove_dir_all(elsewhere).unwrap();
    }
}
