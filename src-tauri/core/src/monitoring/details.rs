//! On-demand details for a single process. These are read only when the user
//! opens a process, so they are not part of the per-tick payload.

use sysinfo::{ProcessRefreshKind, UpdateKind};

use crate::errors::{AppError, AppResult};
use crate::models::ProcessDetails;
use crate::monitoring::Engine;
use crate::platform;

/// `start_time` pins the process instance the user opened; a reused PID is
/// reported as not found.
pub fn process_details(engine: &Engine, pid: u32, start_time: u64) -> AppResult<ProcessDetails> {
    let row = engine
        .process(pid)
        .filter(|row| row.start_time == start_time)
        .ok_or(AppError::ProcessNotFound(pid))?;
    crate::inspect::for_process(pid, row.start_time, || {
        let parent_name = row
            .parent_pid
            .and_then(|ppid| engine.process(ppid))
            .map(|p| p.name);

        // A tiny, single-process query; it does not touch the engine's System.
        let (cwd, total_read, total_written) = platform::with_process(
            pid,
            row.start_time,
            ProcessRefreshKind::nothing()
                .without_tasks()
                .with_cwd(UpdateKind::Always)
                .with_disk_usage(),
            |p| {
                let usage = p.disk_usage();
                (
                    p.cwd()
                        .map(|c| c.to_string_lossy().into_owned())
                        .filter(|c| !c.is_empty()),
                    usage.total_read_bytes,
                    usage.total_written_bytes,
                )
            },
        )?;

        Ok(ProcessDetails {
            history: engine.process_history(pid, row.start_time),
            parent_name,
            cwd,
            architecture: platform::process_architecture(pid),
            // Zero lifetime I/O usually means "not readable", not "no I/O".
            total_read_bytes: Some(total_read).filter(|&b| b > 0),
            total_written_bytes: Some(total_written).filter(|&b| b > 0),
            connections: platform::connections(pid),
            row,
        })
    })
}
