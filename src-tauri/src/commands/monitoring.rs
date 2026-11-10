//! Commands for subscribing to and controlling the monitoring engine.

use std::sync::Arc;

use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Manager, Runtime, State, Webview};

use crate::desktop::{WindowVisibility, update_pause_label};
use crate::state::AppState;
use tm_core::errors::{AppError, AppResult};
use tm_core::models::{
    HistoryPoint, MetricsTick, MonitoringState, StaticSystemInfo, validate_interval,
};
use tm_core::monitoring::TickSink;

/// Longest history window the UI can request.
const MAX_HISTORY_SECONDS: u32 = 3_600;

/// Streams ticks to a webview. While the window is hidden or minimized the
/// engine skips serializing for it; the UI backfills history when shown.
struct ChannelSink {
    channel: Channel,
    visibility: Arc<WindowVisibility>,
}

impl TickSink for ChannelSink {
    fn wants_json(&self) -> bool {
        self.visibility.get()
    }

    fn deliver(&self, _tick: &MetricsTick, json: Option<String>) -> bool {
        match json {
            Some(json) => self.channel.send(InvokeResponseBody::Json(json)).is_ok(),
            None => true,
        }
    }
}

/// Turns per-process shared-memory accounting (PSS) on while the Memory page
/// is open. It costs extra `/proc` reads, so it is off the rest of the time.
#[tauri::command]
pub fn set_memory_detail(state: State<'_, AppState>, enabled: bool) {
    state.engine.set_memory_detail(enabled);
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
    state.engine.subscribe(
        webview.label().to_owned(),
        Arc::new(ChannelSink {
            channel: on_tick,
            visibility: Arc::clone(&state.visibility),
        }),
    );
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

/// Persisting the interval writes the settings file, so this runs off the UI thread.
#[tauri::command]
pub async fn set_monitoring_state<R: Runtime>(
    app: AppHandle<R>,
    paused: bool,
    interval_ms: u32,
) -> AppResult<MonitoringState> {
    let interval_ms = validate_interval(interval_ms)?;
    super::blocking::run(move || {
        let state = app.state::<AppState>();
        let result = state.engine.set_state(MonitoringState {
            paused,
            interval_ms,
        })?;
        // The interval is a persisted setting; pausing is per session.
        if state.settings.get().refresh_interval_ms != interval_ms {
            state.settings.update(
                |s| s.refresh_interval_ms = interval_ms,
                |saved| super::settings::apply_runtime(&state, saved),
            )?;
        }
        update_pause_label(&app, paused);
        Ok(result)
    })
    .await
}

#[tauri::command]
pub fn refresh_now(state: State<'_, AppState>) -> AppResult<()> {
    state.engine.refresh_now()
}
