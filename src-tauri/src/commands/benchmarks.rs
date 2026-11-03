//! Benchmark commands. The frontend can only pick a kind and a profile (both
//! enums); nothing else about a run is controlled from the UI.

use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};

use crate::state::AppState;
use tm_core::benchmarks::coordinator::ProgressSink;
use tm_core::benchmarks::{BenchmarkKind, BenchmarkProfile, BenchmarkResult, BenchmarkStatus};
use tm_core::errors::{AppError, AppResult};

pub const STATUS_EVENT: &str = "benchmark://status";
pub const RESULT_EVENT: &str = "benchmark://result";

/// Forwards progress to the webview as events. The coordinator already limits
/// updates to about ten per second.
struct EventSink(AppHandle);

impl ProgressSink for EventSink {
    fn status(&self, status: &BenchmarkStatus) {
        let _ = self.0.emit(STATUS_EVENT, status);
    }

    fn finished(&self, result: &BenchmarkResult) {
        let _ = self.0.emit(RESULT_EVENT, result);
    }
}

#[tauri::command]
pub fn start_benchmark(
    app: AppHandle,
    state: State<'_, AppState>,
    kind: BenchmarkKind,
    profile: BenchmarkProfile,
) -> AppResult<BenchmarkStatus> {
    state
        .benchmarks
        .start(kind, profile, Arc::new(EventSink(app)))
}

#[tauri::command]
pub fn cancel_benchmark(state: State<'_, AppState>) {
    state.benchmarks.cancel();
}

#[tauri::command]
pub fn get_benchmark_status(state: State<'_, AppState>) -> BenchmarkStatus {
    state.benchmarks.status()
}

#[tauri::command]
pub fn get_benchmark_history(state: State<'_, AppState>) -> Vec<BenchmarkResult> {
    state.benchmarks.history()
}

#[tauri::command]
pub fn delete_benchmark_result(state: State<'_, AppState>, id: String) -> AppResult<()> {
    // Ids are generated as `cpu-<millis>-<n>`; reject anything else.
    let valid = !id.is_empty()
        && id.len() <= 64
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
    if !valid {
        return Err(AppError::InvalidArgument("Not a valid result id.".into()));
    }
    state.benchmarks.delete_result(&id)
}
