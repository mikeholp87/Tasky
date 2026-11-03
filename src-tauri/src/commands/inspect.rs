//! Read-only inspection commands: ports, open files, modules, memory maps,
//! threads and the activity timeline. Each walks OS tables, so it runs on a
//! blocking worker and is called by the UI only while its view is open.

use std::sync::Arc;
use std::time::Instant;

use tauri::{AppHandle, Runtime, State};
use tauri_plugin_dialog::DialogExt;
use tm_core::errors::{AppError, AppResult};
use tm_core::models::{
    ActivityEvent, FileUserList, HardwareInfo, MemoryMap, ModuleInfo, OpenFileList, SocketList,
    ThreadList,
};
use tm_core::monitoring::unix_millis;
use tm_core::platform;

use crate::commands::validate;
use crate::state::AppState;

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| AppError::SystemQueryFailed(format!("background task failed: {e}")))?
}

/// Every TCP/UDP socket with its owning process (Ports page).
#[tauri::command]
pub async fn list_sockets() -> AppResult<SocketList> {
    blocking(platform::sockets).await
}

#[tauri::command]
pub async fn get_open_files(pid: i64) -> AppResult<OpenFileList> {
    let pid = validate::pid(pid)?;
    blocking(move || platform::open_files(pid)).await
}

/// Which processes use a file (or, on Linux, a directory).
#[tauri::command]
pub async fn find_file_users(path: String) -> AppResult<FileUserList> {
    let path = validate::search_path(&path)?.to_owned();
    blocking(move || platform::find_file_users(&path)).await
}

#[tauri::command]
pub async fn get_memory_map(pid: i64) -> AppResult<MemoryMap> {
    let pid = validate::pid(pid)?;
    blocking(move || platform::memory_map(pid)).await
}

#[tauri::command]
pub async fn get_modules(pid: i64) -> AppResult<Vec<ModuleInfo>> {
    let pid = validate::pid(pid)?;
    blocking(move || platform::modules(pid)).await
}

/// Threads with CPU percentages. The percentage needs two readings, so the
/// UI polls this while the Threads tab is visible; the backend keeps the
/// previous reading of one process only.
#[tauri::command]
pub async fn get_threads(state: State<'_, AppState>, pid: i64) -> AppResult<ThreadList> {
    let pid = validate::pid(pid)?;
    let engine = Arc::clone(&state.engine);
    let sampler = Arc::clone(&state.threads);
    blocking(move || {
        // The start time identifies this instance of the PID.
        let start_time = engine
            .process(pid)
            .ok_or(AppError::ProcessNotFound(pid))?
            .start_time;
        let raw = platform::threads(pid)?;
        let cores = engine.static_info().logical_core_count as usize;
        let mut sampler = sampler.lock().unwrap_or_else(|e| e.into_inner());
        Ok(sampler.sample(pid, start_time, raw, Instant::now(), unix_millis(), cores))
    })
    .await
}

/// Timeline events newer than `after` (0 for all kept events).
#[tauri::command]
pub fn get_activity(state: State<'_, AppState>, after: u64) -> Vec<ActivityEvent> {
    state.engine.activity_since(after)
}

/// Native file or folder picker for the file finder. The webview gets only the
/// chosen path, not a general file-system permission. `None` if cancelled.
#[tauri::command]
pub async fn pick_path<R: Runtime>(
    app: AppHandle<R>,
    directory: bool,
) -> AppResult<Option<String>> {
    blocking(move || {
        let dialog = app.dialog().file();
        let picked = if directory {
            dialog.blocking_pick_folder()
        } else {
            dialog.blocking_pick_file()
        };
        let Some(picked) = picked else {
            return Ok(None);
        };
        let path = picked.into_path().map_err(|e| {
            AppError::SystemQueryFailed(format!("cannot read the chosen path: {e}"))
        })?;
        Ok(Some(path.to_string_lossy().into_owned()))
    })
    .await
}

/// Machine identity, CPU caches and batteries for the System Information page.
#[tauri::command]
pub async fn get_hardware_info() -> AppResult<HardwareInfo> {
    blocking(|| Ok(platform::hardware_info())).await
}
