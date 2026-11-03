//! Process control commands. Validation, PID-reuse checks and protection
//! rules live in `tm_core::process_control`; these handlers only adapt IPC.

use super::blocking::run as blocking;
use std::sync::Arc;

use tauri::{AppHandle, State};
use tm_core::errors::{AppError, AppResult};
use tm_core::models::{ActionOutcome, ProcessAction, ProtectionAssessment};
use tm_core::process_control::{self, ActionRequest};

use crate::commands::validate;
use crate::state::AppState;

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
        let assessment = process_control::assess(&engine, req.pid, req.action)?;
        if assessment.level == tm_core::models::ProtectionLevel::Blocked {
            return Err(AppError::ProcessProtected(assessment.reasons.join(" ")));
        }
        if req.action.is_disruptive() || assessment.level == tm_core::models::ProtectionLevel::Warning {
            super::confirm::confirm(&app, &format!(
                "Apply {:?} to process PID {}?\nThis may stop work, lose unsaved data, or make dependent programs unresponsive.\n{}",
                req.action, req.pid, assessment.reasons.join(" ")
            ))?;
        }
        process_control::perform(&engine, req)
    }).await
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
