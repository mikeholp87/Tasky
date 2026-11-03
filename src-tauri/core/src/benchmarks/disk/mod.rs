//! Disk benchmark (`disk-<mode>-v1`): sequential and random I/O on a
//! temporary file, with latency percentiles and queue-depth steps.
//!
//! Methods (see `docs/BENCHMARKS.md`): sequential 1 MiB blocks at queue depth
//! 1; random 4 KiB blocks at queue depths 1, 8 and 32 (reads) and 1 and 32
//! (writes). Direct I/O (`O_DIRECT`) is used when the filesystem allows it, so
//! the drive is measured rather than the page cache.

pub mod aligned;
pub mod device;
pub mod latency;
pub mod pattern;
pub mod plan;
pub mod targets;

#[cfg(unix)]
pub mod engine;
#[cfg(unix)]
pub mod file;

use std::path::PathBuf;

use super::model::{DiskPhaseResult, IoMode};
use plan::DiskConfig;

/// Everything one disk run needs. The coordinator resolves the location from
/// an id; no path ever comes from the frontend.
pub struct DiskRequest {
    /// Folder the temporary directory is created in.
    pub parent: PathBuf,
    /// Lowercase letters, digits and dashes only.
    pub run_id: String,
    pub config: DiskConfig,
    /// Free space at `parent`, for the space-safety check.
    pub free_bytes: u64,
    /// Try direct I/O first.
    pub want_direct: bool,
}

#[derive(Debug)]
pub struct DiskRunOutput {
    pub phases: Vec<DiskPhaseResult>,
    pub io_mode: IoMode,
    pub io_mode_note: Option<String>,
    pub file_bytes: u64,
    pub bytes_written: u64,
    pub bytes_read: u64,
}

/// The disk benchmark needs file APIs that are not implemented here yet.
#[cfg(not(unix))]
pub mod engine {
    use super::super::BenchmarkError;
    use super::super::cancel::CancelToken;
    use super::super::cpu::runner::Observer;
    use super::{DiskRequest, DiskRunOutput};

    pub fn run_disk(
        _req: &DiskRequest,
        _cancel: &CancelToken,
        _observer: &dyn Observer,
    ) -> Result<DiskRunOutput, BenchmarkError> {
        Err(BenchmarkError::Failed(
            "The disk benchmark is not available on this platform yet.".into(),
        ))
    }
}
