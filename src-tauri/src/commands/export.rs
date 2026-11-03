//! Export and diagnostics. Content is generated here from the engine's own
//! snapshot and written to a path the user picks in a native save dialog;
//! the frontend never supplies file contents or paths.

use super::blocking::run as blocking;
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
    if contents.len() > 32 * 1024 * 1024 {
        return Err(AppError::InvalidArgument(
            "Export exceeds the 32 MiB limit.".into(),
        ));
    }
    tm_core::filesystem::write_private_atomic(&path, contents.as_bytes()).map_err(failed)?;
    Ok(Some(path.to_string_lossy().into_owned()))
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
        if include_command_lines {
            super::confirm::confirm(&app, "This export includes process command lines, which may contain passwords or tokens. Continue with this sensitive export?")?;
        }
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
struct Diagnostics {
    generated_at_ms: u64,
    app_version: String,
    system: StaticSystemInfo,
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
    let settings = state.settings.get();
    let recent_log = state.logging.recent();
    blocking(move || {
        if include_processes {
            let message = if include_command_lines {
                "Diagnostics will include process information and command lines, which may contain passwords or tokens, plus hostname, alert rules and recent errors. Continue?"
            } else {
                "Diagnostics will include process information, hostname, alert rules and recent errors, which may be private. Continue?"
            };
            super::confirm::confirm(&app, message)?;
        }
        let mut diagnostics = collect_diagnostics(&engine, settings, recent_log);
        diagnostics.apply_privacy(include_processes, include_command_lines);
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

fn collect_diagnostics(
    engine: &tm_core::monitoring::Engine,
    settings: Settings,
    recent_log: Vec<LogRecord>,
) -> Diagnostics {
    let tick = engine.latest();
    let info = engine.static_info();
    Diagnostics {
        generated_at_ms: tm_core::monitoring::unix_millis(),
        app_version: info.app_version.clone(),
        system: info.clone(),
        monitoring: engine.state(),
        settings,
        last_poll_duration_ms: tick.as_ref().map(|t| t.engine.poll_duration_ms),
        process_count: tick.as_ref().map(|t| t.processes.len()),
        last_payload_bytes: tick.as_ref().map(|t| t.engine.last_payload_bytes),
        recent_log,
        recent_alerts: engine.recent_alerts(),
        processes: tick.as_ref().map(|t| t.processes.clone()),
    }
}

impl Diagnostics {
    fn apply_privacy(&mut self, include_processes: bool, include_command_lines: bool) {
        self.settings.visible_columns.clear();
        self.settings.column_widths.clear();
        if !include_processes {
            self.system.hostname = None;
            self.settings.alert_rules.clear();
            self.recent_log.clear();
            self.recent_alerts.clear();
            self.processes = None;
        } else if !include_command_lines && let Some(processes) = &mut self.processes {
            for process in processes {
                process.cmd = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_diagnostics_exclude_indirect_process_identifiers() {
        let engine = tm_core::monitoring::Engine::start(MonitoringState {
            paused: true,
            interval_ms: 1000,
        })
        .unwrap();
        let logs = vec![LogRecord {
            timestamp_ms: 1,
            level: "WARN".into(),
            target: "test".into(),
            message: "private-process-name".into(),
        }];
        let mut d = collect_diagnostics(&engine, Settings::default(), logs);
        d.system.hostname = Some("private-hostname".into());
        d.settings.alert_rules[0].name = "private-rule-name".into();
        d.recent_alerts.push(AlertEvent {
            rule_id: "private-rule".into(),
            rule_name: "private-process".into(),
            timestamp_ms: 1,
            message: "private-process exceeded its limit".into(),
            pid: Some(42),
        });
        d.processes = Some(Vec::new());
        d.apply_privacy(false, true); // Command-line opt-in cannot override process exclusion.
        let json = serde_json::to_string(&d).unwrap();
        assert!(!json.contains("private-"));
        assert!(d.processes.is_none() && d.recent_alerts.is_empty() && d.recent_log.is_empty());
        engine.shutdown();
    }
}
