//! Process control commands. Validation, PID-reuse checks and protection
//! rules live in `tm_core::process_control`; these handlers only adapt IPC.

use std::sync::Arc;

use tauri::{AppHandle, State};
use tm_core::errors::{AppError, AppResult};
use tm_core::models::{ActionOutcome, PriorityLevel, ProcessAction, ProtectionAssessment};
use tm_core::process_control::{self, ActionRequest};

use crate::commands::validate;
use crate::state::AppState;

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| AppError::SystemQueryFailed(format!("background task failed: {e}")))?
}

/// Protection verdict shown in the confirmation dialog before acting.
#[tauri::command]
pub async fn assess_process_action(
    state: State<'_, AppState>,
    pid: i64,
    action: ProcessAction,
) -> AppResult<ProtectionAssessment> {
    let pid = validate::pid(pid)?;
    let engine = Arc::clone(&state.engine);
    blocking(move || process_control::assess(&engine, pid, action)).await
}

#[tauri::command]
pub async fn perform_process_action(
    app: AppHandle,
    state: State<'_, AppState>,
    pid: i64,
    start_time: u64,
    action: ProcessAction,
    acknowledge_warnings: bool,
) -> AppResult<ActionOutcome> {
    let req = ActionRequest {
        pid: validate::pid(pid)?,
        start_time,
        action,
        acknowledge_warnings,
    };
    let engine = Arc::clone(&state.engine);
    blocking(move || {
        // Name the process in the prompt, and refuse before asking if the PID
        // now belongs to a different process than the one the user saw.
        let row = engine
            .process(req.pid)
            .filter(|r| r.start_time == req.start_time)
            .ok_or(AppError::ProcessNotFound(req.pid))?;
        let assessment = process_control::assess(&engine, req.pid, req.action)?;
        if assessment.level == tm_core::models::ProtectionLevel::Blocked {
            return Err(AppError::ProcessProtected(assessment.reasons.join(" ")));
        }
        if req.action.needs_confirmation()
            || assessment.level == tm_core::models::ProtectionLevel::Warning
        {
            let consequence = match req.action {
                ProcessAction::Resume => {
                    "A process that was stopped on purpose will continue running."
                }
                ProcessAction::SetPriority {
                    priority: PriorityLevel::Idle | PriorityLevel::BelowNormal,
                } => "Lowering priority can make it slow to respond, and only an administrator can raise it again.",
                ProcessAction::SetPriority { .. } => {
                    "Higher priority lets it take CPU time from other programs."
                }
                _ => "This may stop work, lose unsaved data, or make dependent programs unresponsive.",
            };
            super::confirm::confirm(
                &app,
                &format!(
                    "Apply {:?} to {} (PID {})?
{consequence}
{}",
                    req.action,
                    row.name.escape_debug(),
                    req.pid,
                    assessment.reasons.join(" ")
                ),
            )?;
        }
        process_control::perform(&engine, req)
    })
    .await
}

/// Opens the file manager at the process's executable. The path comes from
/// the engine's own snapshot, never from the frontend.
#[tauri::command]
pub async fn reveal_executable(state: State<'_, AppState>, pid: i64) -> AppResult<()> {
    let pid = validate::pid(pid)?;
    let engine = Arc::clone(&state.engine);
    blocking(move || {
        let row = engine.process(pid).ok_or(AppError::ProcessNotFound(pid))?;
        let exe = row.exe.ok_or_else(|| {
            AppError::ActionUnavailable(
                "The executable path of this process is unavailable.".into(),
            )
        })?;
        tauri_plugin_opener::reveal_item_in_dir(&exe).map_err(|e| {
            AppError::SystemQueryFailed(format!("could not open the file manager: {e}"))
        })
    })
    .await
}
