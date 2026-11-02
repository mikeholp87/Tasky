//! Platform abstraction layer.
//!
//! Everything OS-specific lives below this module. Higher-level code calls the
//! functions re-exported here and never uses `#[cfg(target_os)]` itself.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

// Parsers for /proc files are plain functions, compiled on every platform in
// tests so they are exercised by CI on Windows too.
#[cfg(any(target_os = "linux", test))]
pub mod procfs;

#[cfg(target_os = "linux")]
use linux as imp;
#[cfg(target_os = "macos")]
use macos as imp;
#[cfg(target_os = "windows")]
use windows as imp;

/// Memory figures that sysinfo does not expose but the OS reports reliably.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MemoryExtras {
    pub cached_bytes: Option<u64>,
    pub buffers_bytes: Option<u64>,
    pub commit_used_bytes: Option<u64>,
    pub commit_limit_bytes: Option<u64>,
}

/// Best-effort memory extras; failures degrade to "unavailable".
pub fn memory_extras() -> MemoryExtras {
    imp::memory_extras()
}

/// Whether the OS reports load averages. Windows has no equivalent.
pub const HAS_LOAD_AVERAGE: bool = cfg!(unix);
