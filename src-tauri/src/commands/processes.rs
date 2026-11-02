//! Read-only process inspection commands.

use std::sync::Arc;

use tauri::State;

use crate::commands::validate;
use crate::state::AppState;
use tm_core::errors::{AppError, AppResult};
use tm_core::models::ProcessDetails;
use tm_core::monitoring::details;

/// Details for one process. Runs on a blocking worker so OS queries never
/// stall the UI thread.
#[tauri::command]
pub async fn get_process_details(
    state: State<'_, AppState>,
    pid: i64,
) -> AppResult<ProcessDetails> {
    let pid = validate::pid(pid)?;
    let engine = Arc::clone(&state.engine);
    tauri::async_runtime::spawn_blocking(move || details::process_details(&engine, pid))
        .await
        .map_err(|e| AppError::SystemQueryFailed(format!("details task failed: {e}")))?
}
