//! Results of on-demand inspection: sockets, open files, memory maps,
//! modules and threads. None of this is part of the per-tick payload.

use serde::Serialize;

use super::{Connection, ProcessState};

/// A socket with its owning process, for the Ports page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SocketEntry {
    pub connection: Connection,
    /// `None` when the owner cannot be determined (another user's process
    /// without privileges, or a closing socket the kernel no longer attributes).
    pub pid: Option<u32>,
    pub process_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SocketList {
    pub sockets: Vec<SocketEntry>,
    /// Processes whose sockets could not be read. Their sockets show no owner.
    pub inaccessible_processes: u32,
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum OpenFileKind {
    File,
    Directory,
    Socket,
    Pipe,
    Device,
    /// Kernel objects without a path (eventfd, epoll, memfd, ...).
    Anonymous,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum FileAccess {
    Read,
    Write,
    ReadWrite,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct OpenFile {
    /// File descriptor number (Linux) or handle value (Windows).
    pub descriptor: String,
    pub kind: OpenFileKind,
    pub target: String,
    pub access: Option<FileAccess>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct OpenFileList {
    pub files: Vec<OpenFile>,
    pub truncated: bool,
}

/// How a process refers to a searched path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum FileUsage {
    Open,
    WorkingDirectory,
    Executable,
    Mapped,
    /// The OS says the process holds the file but not how (Windows).
    InUse,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct FileUse {
    pub pid: u32,
    pub process_name: String,
    pub usage: FileUsage,
    /// The path as the process refers to it (equal to the search path, or a
    /// path beneath it when a directory was searched).
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct FileUserList {
    /// The path searched, after resolving symlinks.
    pub resolved_path: String,
    pub is_directory: bool,
    pub uses: Vec<FileUse>,
    pub inaccessible_processes: u32,
    /// The search stopped early (time or row limit); results may be incomplete.
    pub truncated: bool,
    /// Platform limits worth showing next to the results.
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum MemoryRegionKind {
    /// The process's main executable.
    Image,
    SharedLibrary,
    MappedFile,
    Heap,
    Stack,
    /// Kernel-provided pages (vdso, vvar, vsyscall).
    Special,
    /// Not backed by a file. Includes thread stacks and malloc arenas, which
    /// the OS does not label.
    Anonymous,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct MemoryRegion {
    /// Hex, without `0x`. Strings because kernel addresses exceed 2^53.
    pub start: String,
    pub end: String,
    pub size: u64,
    /// `rwxp` / `rwxs` style on Linux; `rwx` on Windows.
    pub permissions: String,
    pub offset: u64,
    pub path: Option<String>,
    pub kind: MemoryRegionKind,
    pub shared: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct MemoryKindTotal {
    pub kind: MemoryRegionKind,
    pub bytes: u64,
    pub regions: u32,
}

/// Address-space layout. Sizes are *mapped* (virtual) bytes, not resident.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct MemoryMap {
    pub regions: Vec<MemoryRegion>,
    /// Per-kind totals, largest first. Empty when there is nothing to chart.
    pub summary: Vec<MemoryKindTotal>,
    pub total_bytes: u64,
    pub executable_bytes: u64,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ModuleInfo {
    pub name: String,
    pub path: String,
    /// Lowest mapped address, hex without `0x`.
    pub base: String,
    /// Windows: image size. Linux: total mapped bytes of the file.
    pub size: u64,
    pub executable: bool,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ThreadInfo {
    pub tid: u32,
    pub name: Option<String>,
    pub state: Option<ProcessState>,
    /// Share of the whole machine, like the Processes page. `None` on the
    /// first sample, which has no previous reading to compare with.
    pub cpu_percent: Option<f32>,
    pub cpu_time_ms: u64,
    pub priority: Option<i32>,
    /// Unix seconds.
    pub start_time: Option<u64>,
    /// CPU the thread last ran on.
    pub processor: Option<u32>,
    /// Kernel function the thread is blocked in, when readable (Linux).
    pub wait_channel: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ThreadList {
    pub threads: Vec<ThreadInfo>,
    /// Unix milliseconds.
    pub sampled_at_ms: u64,
}
