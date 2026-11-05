//! Privileged process actions. Every action passes through here:
//!
//! 1. the target must still be the process the user saw (PID + start time),
//! 2. protection rules are evaluated server-side (the UI cannot skip them),
//! 3. warnings require explicit acknowledgement,
//! 4. only then is the platform operation performed.

pub mod protection;

use crate::errors::{AppError, AppResult};
use crate::models::{
    ActionOutcome, ActivityKind, Platform, ProcessAction, ProcessRow, ProtectionAssessment,
    ProtectionLevel,
};
use crate::monitoring::Engine;
use crate::monitoring::activity::Pending;
use crate::platform;
use protection::ProcessFacts;

#[derive(Debug, Clone, Copy)]
pub struct ActionRequest {
    pub pid: u32,
    /// Start time the UI saw; guards against acting on a reused PID.
    pub start_time: u64,
    pub action: ProcessAction,
    pub acknowledge_warnings: bool,
}

/// Evaluates protection rules for `action` on the current process `pid`.
pub fn assess(engine: &Engine, pid: u32, action: ProcessAction) -> AppResult<ProtectionAssessment> {
    let row = engine.process(pid).ok_or(AppError::ProcessNotFound(pid))?;
    Ok(assess_row(engine, &row, action))
}

fn assess_row(engine: &Engine, row: &ProcessRow, action: ProcessAction) -> ProtectionAssessment {
    let system_root = platform::system_root();
    let facts = ProcessFacts {
        pid: row.pid,
        parent_pid: row.parent_pid,
        name: &row.name,
        exe: row.exe.as_deref(),
        user: row.user.as_deref(),
        self_pid: engine.static_info().self_pid,
        // Only worth an extra OS call for actions that could crash Windows.
        is_critical: action
            .is_disruptive()
            .then(|| platform::is_critical(row.pid))
            .flatten(),
        system_root: system_root.as_deref(),
        kernel_thread: row.kernel_thread,
        zombie: row.status == crate::models::ProcessState::Zombie,
    };
    protection::assess(Platform::current(), &facts, action)
}

pub fn perform(engine: &Engine, req: ActionRequest) -> AppResult<ActionOutcome> {
    let row = engine
        .process(req.pid)
        .filter(|r| r.start_time == req.start_time)
        // Exited, or the PID now belongs to a different process.
        .ok_or(AppError::ProcessNotFound(req.pid))?;

    let assessment = assess_row(engine, &row, req.action);
    match assessment.level {
        ProtectionLevel::Blocked => {
            return Err(AppError::ProcessProtected(assessment.reasons.join(" ")));
        }
        ProtectionLevel::Warning if !req.acknowledge_warnings => {
            return Err(AppError::ProcessProtected(format!(
                "Confirmation required: {}",
                assessment.reasons.join(" ")
            )));
        }
        _ => {}
    }

    platform::perform_action(req.pid, req.start_time, req.action)?;

    let pid = req.pid;
    let name = &row.name;
    let message = match req.action {
        ProcessAction::Terminate => {
            if cfg!(windows) {
                format!("Asked {name} (PID {pid}) to close. It may prompt to save first.")
            } else {
                format!("Sent a termination request to {name} (PID {pid}).")
            }
        }
        ProcessAction::ForceKill => {
            format!("Ended {name} (PID {pid}).")
        }
        ProcessAction::Suspend => {
            engine.set_suspended(pid, row.start_time, true);
            format!("Suspended {name} (PID {pid}).")
        }
        ProcessAction::Resume => {
            engine.set_suspended(pid, row.start_time, false);
            format!("Resumed {name} (PID {pid}).")
        }
        ProcessAction::SetPriority { .. } => {
            format!("Changed the priority of {name} (PID {pid}).")
        }
    };
    // Process identifiers can contain private or misleading text.
    tracing::info!(pid, action = ?req.action, "process action performed");
    let what = match req.action {
        ProcessAction::Terminate => "Asked to close",
        ProcessAction::ForceKill => "Force ended",
        ProcessAction::Suspend => "Suspended",
        ProcessAction::Resume => "Resumed",
        ProcessAction::SetPriority { .. } => "Priority changed",
    };
    engine.record_activity(
        Pending::process(ActivityKind::ProcessAction, pid, name)
            .detail(format!("{what} from this app")),
    );
    // Show the effect promptly rather than at the next scheduled tick.
    let _ = engine.refresh_now();
    Ok(ActionOutcome { message })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{MonitoringState, ProcessState};
    use std::process::{Child, Command, Stdio};
    use std::thread;
    use std::time::{Duration, Instant};

    /// A long-running, windowless child that this test owns.
    fn spawn_child() -> Child {
        let mut cmd = if cfg!(windows) {
            let mut c = Command::new("ping");
            c.args(["-n", "60", "127.0.0.1"]);
            c
        } else {
            let mut c = Command::new("sleep");
            c.arg("60");
            c
        };
        cmd.stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn child")
    }

    fn wait_for_row(engine: &Engine, pid: u32, cond: impl Fn(&ProcessRow) -> bool) -> ProcessRow {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            engine.refresh_now().unwrap();
            thread::sleep(Duration::from_millis(250));
            if let Some(row) = engine.process(pid).filter(|r| cond(r)) {
                return row;
            }
            assert!(Instant::now() < deadline, "condition not met for pid {pid}");
        }
    }

    #[test]
    fn controls_a_spawned_child_end_to_end() {
        let engine = Engine::start(MonitoringState {
            paused: true,
            interval_ms: 500,
        })
        .unwrap();
        let mut child = spawn_child();
        let pid = child.id();
        let row = wait_for_row(&engine, pid, |_| true);
        let req = |action, start_time| ActionRequest {
            pid,
            start_time,
            action,
            acknowledge_warnings: false,
        };

        // A stale start time (PID reuse) is refused.
        let stale = perform(&engine, req(ProcessAction::ForceKill, row.start_time + 1));
        assert!(matches!(stale, Err(AppError::ProcessNotFound(_))));

        // The OS boundary must reject a mismatched lifetime independently of
        // the monitoring cache. No signal is sent to this disposable child.
        assert!(matches!(
            platform::perform_action(pid, row.start_time + 1, ProcessAction::ForceKill),
            Err(AppError::ProcessNotFound(_))
        ));
        assert!(child.try_wait().unwrap().is_none());

        perform(&engine, req(ProcessAction::Suspend, row.start_time)).unwrap();
        wait_for_row(&engine, pid, |r| r.status == ProcessState::Stopped);

        perform(&engine, req(ProcessAction::Resume, row.start_time)).unwrap();
        wait_for_row(&engine, pid, |r| r.status != ProcessState::Stopped);

        if cfg!(windows) {
            // A windowless console program cannot be asked to close gracefully.
            let graceful = perform(&engine, req(ProcessAction::Terminate, row.start_time));
            assert!(matches!(graceful, Err(AppError::ActionUnavailable(_))));
        }

        perform(&engine, req(ProcessAction::ForceKill, row.start_time)).unwrap();
        let status = child.wait().expect("child exits");
        assert!(!status.success(), "killed child should not exit cleanly");
        engine.shutdown();
    }

    #[test]
    fn refuses_to_act_on_itself() {
        let engine = Engine::start(MonitoringState {
            paused: true,
            interval_ms: 500,
        })
        .unwrap();
        let me = std::process::id();
        let row = wait_for_row(&engine, me, |_| true);
        let result = perform(
            &engine,
            ActionRequest {
                pid: me,
                start_time: row.start_time,
                action: ProcessAction::ForceKill,
                acknowledge_warnings: true,
            },
        );
        assert!(matches!(result, Err(AppError::ProcessProtected(_))));
        engine.shutdown();
    }
}
