//! Settings and alert commands.

use std::sync::atomic::Ordering;

use tauri::{AppHandle, Runtime, State};
use tm_core::errors::AppResult;
use tm_core::models::{AlertEvent, MonitoringState, Settings};
use tm_core::monitoring::engine::MAX_HISTORY_POINTS;

use crate::state::AppState;

#[tauri::command]
pub fn get_settings<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> AppResult<Settings> {
    let mut settings = state.settings.get();
    settings.start_with_os = crate::autostart::is_enabled(&app)?;
    Ok(settings)
}

/// Rolls autostart back if persistence fails; one store lock covers both changes.
#[tauri::command]
pub fn update_settings<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    settings: Settings,
) -> AppResult<Settings> {
    save_settings(&app, &state, settings)
}

#[tauri::command]
pub fn reset_settings<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> AppResult<Settings> {
    save_settings(&app, &state, Settings::default())
}

pub fn save_settings<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    settings: Settings,
) -> AppResult<Settings> {
    let saved = state.settings.save_with_autostart(
        settings,
        || crate::autostart::is_enabled(app),
        |enabled| crate::autostart::set(app, enabled),
    )?;
    apply_runtime(state, &saved);
    Ok(saved)
}

#[tauri::command]
pub fn get_recent_alerts(state: State<'_, AppState>) -> Vec<AlertEvent> {
    state.engine.recent_alerts()
}

/// Pushes settings that affect running components into them.
pub fn apply_runtime(state: &AppState, s: &Settings) {
    let engine = &state.engine;
    let current = engine.state();
    if current.interval_ms != s.refresh_interval_ms {
        let _ = engine.set_state(MonitoringState {
            paused: current.paused,
            interval_ms: s.refresh_interval_ms,
        });
    }
    engine.set_history_capacity(history_points(s));
    engine.set_alert_rules(s.alert_rules.clone());
    engine.set_include_kernel_threads(s.show_kernel_threads);
    engine.set_activity_config(s.cpu_spike_percent, s.memory_jump_mb);
    state
        .notifications_enabled
        .store(s.notifications_enabled, Ordering::Relaxed);
    state.logging.set_debug(s.debug_logging);
}

/// Points needed to cover the configured history length at the configured
/// interval, bounded by the engine's maximum.
pub fn history_points(s: &Settings) -> usize {
    let per_minute = 60_000 / s.refresh_interval_ms.max(1) as usize;
    (s.history_minutes as usize * per_minute).min(MAX_HISTORY_POINTS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_points_cover_the_window() {
        let s = Settings {
            refresh_interval_ms: 1_000,
            history_minutes: 15,
            ..Settings::default()
        };
        assert_eq!(history_points(&s), 900);
        let fast = Settings {
            refresh_interval_ms: 500,
            history_minutes: 60,
            ..Settings::default()
        };
        assert_eq!(history_points(&fast), MAX_HISTORY_POINTS);
    }
}
