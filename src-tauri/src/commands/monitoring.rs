//! Commands for subscribing to and controlling the monitoring engine.

use std::sync::Arc;

use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{State, Webview};

use crate::state::AppState;
use tm_core::errors::{AppError, AppResult};
use tm_core::models::{
    HistoryPoint, MetricsTick, MonitoringState, StaticSystemInfo, validate_interval,
};
use tm_core::monitoring::TickSink;

/// Longest history window the UI can request.
const MAX_HISTORY_SECONDS: u32 = 3_600;

struct ChannelSink(Channel);

impl TickSink for ChannelSink {
    fn deliver(&self, json: &str) -> bool {
        self.0
            .send(InvokeResponseBody::Json(json.to_owned()))
            .is_ok()
    }
}

#[tauri::command]
pub fn get_static_info(state: State<'_, AppState>) -> StaticSystemInfo {
    state.engine.static_info().clone()
}

/// Streams every future tick to `on_tick`. One subscription per webview;
/// subscribing again (e.g. after a reload) replaces the previous channel.
/// Returns the most recent tick so the UI can render immediately.
#[tauri::command]
pub fn subscribe_metrics(
    webview: Webview,
    state: State<'_, AppState>,
    on_tick: Channel,
) -> Option<MetricsTick> {
    state
        .engine
        .subscribe(webview.label().to_owned(), Arc::new(ChannelSink(on_tick)));
    state.engine.latest().map(|t| (*t).clone())
}

#[tauri::command]
pub fn get_history(state: State<'_, AppState>, seconds: u32) -> AppResult<Vec<HistoryPoint>> {
    if seconds == 0 || seconds > MAX_HISTORY_SECONDS {
        return Err(AppError::InvalidArgument(format!(
            "History range must be between 1 and {MAX_HISTORY_SECONDS} seconds."
        )));
    }
    Ok(state.engine.history(seconds))
}

#[tauri::command]
pub fn get_monitoring_state(state: State<'_, AppState>) -> MonitoringState {
    state.engine.state()
}

#[tauri::command]
pub fn set_monitoring_state(
    state: State<'_, AppState>,
    paused: bool,
    interval_ms: u32,
) -> AppResult<MonitoringState> {
    let interval_ms = validate_interval(interval_ms)?;
    state.engine.set_state(MonitoringState {
        paused,
        interval_ms,
    })
}

#[tauri::command]
pub fn refresh_now(state: State<'_, AppState>) -> AppResult<()> {
    state.engine.refresh_now()
}
