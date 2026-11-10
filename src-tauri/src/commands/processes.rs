//! Read-only process inspection commands.

use std::sync::Arc;

use tauri::State;

use crate::commands::validate;
use crate::state::AppState;
use tm_core::errors::AppResult;
use tm_core::models::ProcessDetails;
use tm_core::monitoring::details;

/// Details for one process instance, identified by PID and start time so a
/// reused PID is never shown as the process the user opened. Runs on a
/// blocking worker so OS queries never stall the UI thread.
#[tauri::command]
pub async fn get_process_details(
    state: State<'_, AppState>,
    pid: i64,
    start_time: u64,
) -> AppResult<ProcessDetails> {
    let pid = validate::pid(pid)?;
    let engine = Arc::clone(&state.engine);
    super::blocking::run(move || details::process_details(&engine, pid, start_time)).await
}
