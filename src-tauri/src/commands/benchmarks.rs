//! Benchmark commands. All arguments are validated in `tm-core`
//! (`validate_request`, history id checks); nothing here accepts paths,
//! URLs or commands from the frontend.

use std::sync::Arc;

use tauri::State;
use tauri::ipc::Channel;
use tm_core::benchmarks::models::{
    BenchmarkCapabilities, BenchmarkEvent, BenchmarkRecord, BenchmarkRequest, BenchmarkSummary,
    PreflightReport,
};
use tm_core::benchmarks::progress::ProgressSink;
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
