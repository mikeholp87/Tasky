//! Export and diagnostics. Content is generated here from the engine's own
//! snapshot and written to a path the user picks in a native save dialog;
//! the frontend never supplies file contents or paths.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime, State};
use tauri_plugin_dialog::DialogExt;
use tm_core::errors::{AppError, AppResult};
use tm_core::export;
use tm_core::models::{AlertEvent, MonitoringState, ProcessRow, Settings, StaticSystemInfo};

use crate::logging::LogRecord;
use crate::state::AppState;

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportFormat {
    Csv,
    Json,
}

fn failed(e: impl std::fmt::Display) -> AppError {
    AppError::SystemQueryFailed(format!("export failed: {e}"))
}

/// Asks for a destination and writes `contents`. Returns the saved path, or
/// `None` if the user cancelled.
fn save_with_dialog<R: Runtime>(
    app: &AppHandle<R>,
    default_name: &str,
    filter: (&str, &str),
    contents: String,
) -> AppResult<Option<String>> {
    let Some(file) = app
        .dialog()
        .file()
        .add_filter(filter.0, &[filter.1])
        .set_file_name(default_name)
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let path = file.into_path().map_err(failed)?;
    std::fs::write(&path, contents).map_err(failed)?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(failed)?
}

fn timestamp() -> String {
    // Seconds are enough to make names unique and sortable.
    tm_core::monitoring::unix_millis()
        .div_euclid(1000)
        .to_string()
}

#[tauri::command]
pub async fn export_processes<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    format: ExportFormat,
    include_command_lines: bool,
) -> AppResult<Option<String>> {
    let engine = Arc::clone(&state.engine);
    blocking(move || {
        let tick = engine.latest().ok_or(AppError::EngineUnavailable)?;
        let (contents, ext, label) = match format {
            ExportFormat::Csv => (
                export::processes_csv(&tick.processes, include_command_lines),
                "csv",
                "CSV",
            ),
            ExportFormat::Json => (
                export::processes_json(&tick.processes, include_command_lines).map_err(failed)?,
                "json",
                "JSON",
            ),
        };
        save_with_dialog(
            &app,
            &format!("processes-{}.{ext}", timestamp()),
            (label, ext),
            contents,
        )
    })
    .await
}

#[tauri::command]
pub async fn export_system_summary<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> AppResult<Option<String>> {
    let engine = Arc::clone(&state.engine);
    blocking(move || {
        let tick = engine.latest().ok_or(AppError::EngineUnavailable)?;
        let json = export::system_summary_json(engine.static_info(), &tick).map_err(failed)?;
        save_with_dialog(
            &app,
            &format!("system-summary-{}.json", timestamp()),
            ("JSON", "json"),
            json,
        )
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Diagnostics<'a> {
    generated_at_ms: u64,
    app_version: &'a str,
    system: &'a StaticSystemInfo,
    monitoring: MonitoringState,
    settings: Settings,
    last_poll_duration_ms: Option<f64>,
    process_count: Option<usize>,
    last_payload_bytes: Option<u32>,
    recent_log: Vec<LogRecord>,
    recent_alerts: Vec<AlertEvent>,
    /// Present only when the user opted in.
    processes: Option<Vec<ProcessRow>>,
}

/// Bundles information useful for bug reports. Process names and command
/// lines are private and included only when explicitly selected.
#[tauri::command]
pub async fn export_diagnostics<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    include_processes: bool,
    include_command_lines: bool,
) -> AppResult<Option<String>> {
    let engine = Arc::clone(&state.engine);
    let mut settings = state.settings.get();
    // Column layout is UI noise, not diagnostic information.
    settings.visible_columns.clear();
    settings.column_widths.clear();
    let recent_log = state.logging.recent();
    blocking(move || {
        let tick = engine.latest();
        let processes = include_processes.then(|| {
            tick.as_ref()
                .map(|t| {
                    t.processes
                        .iter()
                        .cloned()
                        .map(|mut p| {
                            if !include_command_lines {
                                p.cmd = None;
                            }
                            p
                        })
                        .collect()
                })
                .unwrap_or_default()
        });
        let info = engine.static_info();
        let diagnostics = Diagnostics {
            generated_at_ms: tm_core::monitoring::unix_millis(),
            app_version: &info.app_version,
            system: info,
            monitoring: engine.state(),
            settings,
            last_poll_duration_ms: tick.as_ref().map(|t| t.engine.poll_duration_ms),
            process_count: tick.as_ref().map(|t| t.processes.len()),
            last_payload_bytes: tick.as_ref().map(|t| t.engine.last_payload_bytes),
            recent_log,
            recent_alerts: engine.recent_alerts(),
            processes,
        };
        let json = serde_json::to_string_pretty(&diagnostics).map_err(failed)?;
        save_with_dialog(
            &app,
            &format!("taskmanager-diagnostics-{}.json", timestamp()),
            ("JSON", "json"),
            json,
        )
    })
    .await
}
