//! Benchmark commands. All arguments are validated in `tm-core`
//! (`validate_request`, history id checks); nothing here accepts paths,
//! URLs or commands from the frontend.

use std::sync::Arc;

use serde::Deserialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Runtime, State};
use tm_core::benchmarks::models::{
    BenchmarkCapabilities, BenchmarkComparison, BenchmarkEvent, BenchmarkRecord, BenchmarkRequest,
    BenchmarkSummary, NetworkProgress, NetworkSubmission, PreflightReport,
};
use tm_core::benchmarks::progress::ProgressSink;
use tm_core::benchmarks::report;
use tm_core::errors::{AppError, AppResult};

use crate::state::AppState;

/// Forwards benchmark events to the webview that started the run.
struct ChannelSink(Channel<BenchmarkEvent>);

impl ProgressSink for ChannelSink {
    fn send(&self, event: &BenchmarkEvent) -> bool {
        self.0.send(event.clone()).is_ok()
    }
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| AppError::SystemQueryFailed(format!("background task failed: {e}")))?
}

fn run_id(raw: &str) -> AppResult<&str> {
    if raw.is_empty() || raw.len() > 64 {
        return Err(AppError::InvalidArgument("Unknown benchmark run.".into()));
    }
    Ok(raw)
}

/// Async because the first call probes temperature sensors.
#[tauri::command]
pub async fn get_benchmark_capabilities(
    state: State<'_, AppState>,
) -> AppResult<BenchmarkCapabilities> {
    let benchmarks = Arc::clone(&state.benchmarks);
    blocking(move || Ok(benchmarks.capabilities())).await
}

#[tauri::command]
pub async fn run_benchmark_preflight(
    state: State<'_, AppState>,
    request: BenchmarkRequest,
) -> AppResult<PreflightReport> {
    let benchmarks = Arc::clone(&state.benchmarks);
    blocking(move || benchmarks.preflight(request)).await
}

/// Starts a run and streams its events to `on_event`. Returns the run id.
#[tauri::command]
pub fn start_benchmark(
    state: State<'_, AppState>,
    request: BenchmarkRequest,
    on_event: Channel<BenchmarkEvent>,
) -> AppResult<String> {
    state
        .benchmarks
        .start(request, Arc::new(ChannelSink(on_event)))
}

#[tauri::command]
pub fn cancel_benchmark(state: State<'_, AppState>, run_id: String) -> AppResult<bool> {
    Ok(state.benchmarks.cancel(self::run_id(&run_id)?))
}

#[tauri::command]
pub fn skip_benchmark_cooldown(state: State<'_, AppState>, run_id: String) -> AppResult<bool> {
    Ok(state.benchmarks.skip_cooldown(self::run_id(&run_id)?))
}

#[tauri::command]
pub async fn list_benchmark_history(
    state: State<'_, AppState>,
) -> AppResult<Vec<BenchmarkSummary>> {
    let benchmarks = Arc::clone(&state.benchmarks);
    blocking(move || Ok(benchmarks.history().list())).await
}

#[tauri::command]
pub async fn get_benchmark_result(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<BenchmarkRecord> {
    let benchmarks = Arc::clone(&state.benchmarks);
    blocking(move || benchmarks.history().get(&id)).await
}

#[tauri::command]
pub async fn delete_benchmark_result(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let benchmarks = Arc::clone(&state.benchmarks);
    blocking(move || benchmarks.history().delete(&id)).await
}

/// Progress from the WebView internet-test provider. `token` must match the
/// running test's request.
#[tauri::command]
pub fn report_network_progress(
    state: State<'_, AppState>,
    token: String,
    progress: NetworkProgress,
) -> AppResult<()> {
    state.network_inbox.progress(&token, progress)
}

/// Final raw samples from the WebView provider; validated and analyzed in
/// `tm-core` (nothing is trusted from the WebView).
#[tauri::command]
pub async fn submit_network_samples(
    state: State<'_, AppState>,
    token: String,
    samples: NetworkSubmission,
) -> AppResult<()> {
    let inbox = Arc::clone(&state.network_inbox);
    blocking(move || inbox.submit(&token, samples)).await
}

#[tauri::command]
pub async fn compare_benchmarks(
    state: State<'_, AppState>,
    baseline_id: String,
    current_id: String,
) -> AppResult<BenchmarkComparison> {
    let benchmarks = Arc::clone(&state.benchmarks);
    blocking(move || {
        let history = benchmarks.history();
        Ok(report::compare(
            &history.get(&baseline_id)?,
            &history.get(&current_id)?,
        ))
    })
    .await
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BenchmarkExportFormat {
    Json,
    Csv,
    Markdown,
}

/// Exports a stored result through a native save dialog. Returns the saved
/// path, or `None` if the user cancelled.
#[tauri::command]
pub async fn export_benchmark<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    id: String,
    format: BenchmarkExportFormat,
) -> AppResult<Option<String>> {
    let benchmarks = Arc::clone(&state.benchmarks);
    blocking(move || {
        let record = benchmarks.history().get(&id)?;
        let (contents, ext, label) = match format {
            BenchmarkExportFormat::Json => (
                serde_json::to_string_pretty(&record)
                    .map_err(|e| AppError::SystemQueryFailed(format!("export failed: {e}")))?,
                "json",
                "JSON",
            ),
            BenchmarkExportFormat::Csv => (report::to_csv(&record), "csv", "CSV"),
            BenchmarkExportFormat::Markdown => (report::to_markdown(&record), "md", "Markdown"),
        };
        let name = format!("benchmark-{}.{ext}", record.started_at_ms / 1_000);
        crate::commands::export::save_with_dialog(&app, &name, (label, ext), contents)
    })
    .await
}

/// Plain-text summary for "Copy benchmark summary".
#[tauri::command]
pub async fn benchmark_summary_text(state: State<'_, AppState>, id: String) -> AppResult<String> {
    let benchmarks = Arc::clone(&state.benchmarks);
    blocking(move || Ok(report::summary_text(&benchmarks.history().get(&id)?))).await
}
