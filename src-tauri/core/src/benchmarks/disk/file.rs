//! The temporary benchmark file and its cleanup.
//!
//! * Names are `.taskmanager-benchmark-<uuid v4>.tmp`, created with
//!   create-new semantics, so an existing file is never overwritten.
//! * [`TempBenchFile`] deletes the file when dropped: after completion,
//!   cancellation, errors and panics that unwind.
//! * [`sweep`] removes files abandoned by a crash. It only deletes regular
//!   files whose name matches the exact pattern, inside known benchmark
//!   folders, never recursing and never following symlinks.

use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

use crate::benchmarks::history::is_valid_id;
use crate::platform;

pub const FILE_PREFIX: &str = ".taskmanager-benchmark-";
pub const FILE_SUFFIX: &str = ".tmp";
/// Alignment for direct I/O buffers, offsets and sizes.
pub const ALIGN: usize = 4096;

pub fn is_benchmark_file_name(name: &str) -> bool {
    name.strip_prefix(FILE_PREFIX)
        .and_then(|rest| rest.strip_suffix(FILE_SUFFIX))
        .is_some_and(is_valid_id)
}

pub struct TempBenchFile {
    path: PathBuf,
}

impl TempBenchFile {
    /// Creates a new, empty benchmark file in `dir`.
    pub fn create(dir: &Path) -> io::Result<Self> {
        let name = format!("{FILE_PREFIX}{}{FILE_SUFFIX}", uuid::Uuid::new_v4());
        let path = dir.join(name);
        // Create (never reuse) with plain options, then close; handles for
        // the actual I/O are opened separately with the chosen mode.
        drop(platform::open_benchmark_file(&path, true, false)?);
        Ok(Self { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Opens a handle, trying direct I/O first when requested. Returns the
    /// handle and whether it is direct.
    pub fn open(&self, direct: bool) -> io::Result<(File, bool)> {
        if direct && let Ok(f) = platform::open_benchmark_file(&self.path, false, true) {
            return Ok((f, true));
        }
        Ok((
            platform::open_benchmark_file(&self.path, false, false)?,
            false,
        ))
    }
}

impl Drop for TempBenchFile {
    fn drop(&mut self) {
        match std::fs::remove_file(&self.path) {
            Ok(()) => tracing::debug!("removed benchmark file"),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => tracing::warn!("could not remove benchmark file: {e}"),
        }
    }
}

/// Deletes abandoned benchmark files in `dirs`. Returns how many were removed.
pub fn sweep(dirs: &[PathBuf]) -> usize {
    let mut removed = 0;
    for dir in dirs {
        let Ok(meta) = std::fs::symlink_metadata(dir) else {
            continue;
        };
        if !meta.is_dir() || meta.file_type().is_symlink() {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if !is_benchmark_file_name(name) {
                continue;
            }
            let path = entry.path();
            let is_file = std::fs::symlink_metadata(&path).is_ok_and(|m| m.is_file());
            if is_file && std::fs::remove_file(&path).is_ok() {
                removed += 1;
            }
        }
    }
    if removed > 0 {
        tracing::info!(removed, "removed abandoned benchmark files");
    }
    removed
}

/// A heap buffer whose usable slice starts at an `ALIGN`-byte boundary, as
/// direct I/O requires. Built from a plain `Vec`; no unsafe code.
pub struct AlignedBuf {
    raw: Vec<u8>,
    start: usize,
    len: usize,
}

impl AlignedBuf {
    pub fn new(len: usize) -> Self {
        let raw = vec![0u8; len + ALIGN];
        let start = raw.as_ptr().align_offset(ALIGN);
        Self { raw, start, len }
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.raw[self.start..self.start + self.len]
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.raw[self.start..self.start + self.len]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("tm-bench-file-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn name_pattern_is_strict() {
        let good = format!("{FILE_PREFIX}{}{FILE_SUFFIX}", uuid::Uuid::new_v4());
        assert!(is_benchmark_file_name(&good));
        for bad in [
            ".taskmanager-benchmark-.tmp",
            ".taskmanager-benchmark-notauuid.tmp",
            "taskmanager-benchmark-00000000-0000-0000-0000-000000000000.tmp",
            ".taskmanager-benchmark-00000000-0000-0000-0000-000000000000.tmp.bak",
            "important.docx",
        ] {
            assert!(!is_benchmark_file_name(bad), "{bad}");
        }
    }

    #[test]
    fn file_is_removed_on_drop_even_after_errors() {
        let dir = temp();
        let path = {
            let f = TempBenchFile::create(&dir).unwrap();
            let (handle, _) = f.open(true).unwrap();
            drop(handle);
            f.path().to_path_buf()
        };
        assert!(!path.exists());
        let result: Result<(), ()> = (|| {
            let _f = TempBenchFile::create(&dir).map_err(|_| ())?;
            Err(())
        })();
        assert!(result.is_err());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn sweep_deletes_only_matching_files() {
        let dir = temp();
        let abandoned = dir.join(format!(
            "{FILE_PREFIX}{}{FILE_SUFFIX}",
            uuid::Uuid::new_v4()
        ));
        std::fs::write(&abandoned, b"x").unwrap();
        std::fs::write(dir.join("notes.txt"), b"keep").unwrap();
        std::fs::write(dir.join(".taskmanager-benchmark-x.tmp"), b"keep").unwrap();
        let nested = dir.join(format!(
            "{FILE_PREFIX}{}{FILE_SUFFIX}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir(&nested).unwrap(); // a directory with a matching name
        assert_eq!(sweep(std::slice::from_ref(&dir)), 1);
        assert!(!abandoned.exists());
        assert!(dir.join("notes.txt").exists());
        assert!(nested.exists());
        assert_eq!(sweep(&[dir.join("missing")]), 0);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn aligned_buffer_is_aligned() {
        let mut b = AlignedBuf::new(8192);
        assert_eq!(b.as_slice().as_ptr() as usize % ALIGN, 0);
        assert_eq!(b.as_mut_slice().len(), 8192);
    }
}
