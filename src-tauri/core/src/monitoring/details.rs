//! On-demand details for a single process. These are read only when the user
//! opens a process, so they are not part of the per-tick payload.

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

use crate::errors::{AppError, AppResult};
use crate::models::ProcessDetails;
use crate::monitoring::Engine;
use crate::platform;

pub fn process_details(engine: &Engine, pid: u32) -> AppResult<ProcessDetails> {
    let row = engine.process(pid).ok_or(AppError::ProcessNotFound(pid))?;
    let parent_name = row
        .parent_pid
        .and_then(|ppid| engine.process(ppid))
        .map(|p| p.name);

    // A tiny, single-process query; it does not touch the engine's System.
    let (cwd, total_read, total_written) = match System::new() {
        Ok(mut sys) => {
            let target = [Pid::from_u32(pid)];
            sys.refresh_processes_specifics(
                ProcessesToUpdate::Some(&target),
                true,
                ProcessRefreshKind::nothing()
                    .without_tasks()
                    .with_cwd(UpdateKind::Always)
                    .with_disk_usage(),
            );
            match sys.process(Pid::from_u32(pid)) {
                Some(p) if p.start_time() == row.start_time => {
                    let usage = p.disk_usage();
                    (
                        p.cwd()
                            .map(|c| c.to_string_lossy().into_owned())
                            .filter(|c| !c.is_empty()),
                        Some(usage.total_read_bytes),
                        Some(usage.total_written_bytes),
                    )
                }
                // Exited between the engine tick and this query.
                _ => return Err(AppError::ProcessNotFound(pid)),
            }
        }
        Err(_) => (None, None, None),
    };

    Ok(ProcessDetails {
        history: engine.process_history(pid, row.start_time),
        parent_name,
        cwd,
        architecture: platform::process_architecture(pid),
        // Zero lifetime I/O usually means "not readable", not "no I/O".
        total_read_bytes: total_read.filter(|&b| b > 0),
        total_written_bytes: total_written.filter(|&b| b > 0),
        connections: platform::connections(pid),
        row,
    })
}
