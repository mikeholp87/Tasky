//! Settings and alert commands.

use std::sync::atomic::Ordering;

use tauri::{AppHandle, Manager, Runtime, State};
use tm_core::errors::AppResult;
use tm_core::models::{AlertEvent, MonitoringState, Settings};
use tm_core::monitoring::engine::MAX_HISTORY_POINTS;

use super::blocking::run as blocking;
use crate::state::AppState;

/// Reading the login entry touches the disk, so this runs off the UI thread.
#[tauri::command]
pub async fn get_settings<R: Runtime>(app: AppHandle<R>) -> AppResult<Settings> {
    blocking(move || {
        let mut settings = app.state::<AppState>().settings.get();
        settings.start_with_os = crate::autostart::is_enabled(&app)?;
        Ok(settings)
    })
    .await
}

/// Rolls autostart back if persistence fails; one store lock covers both changes.
#[tauri::command]
pub async fn update_settings<R: Runtime>(
    app: AppHandle<R>,
    settings: Settings,
) -> AppResult<Settings> {
    blocking(move || {
        confirm_autostart_change(&app, settings.start_with_os)?;
        save_settings(&app, |s| *s = settings)
    })
    .await
}

#[tauri::command]
pub async fn reset_settings<R: Runtime>(app: AppHandle<R>) -> AppResult<Settings> {
    blocking(move || {
        let defaults = Settings::default();
        confirm_autostart_change(&app, defaults.start_with_os)?;
        save_settings(&app, |s| *s = defaults)
    })
    .await
}

/// Starting Tasky at login is a startup entry like any other, so changing it
/// asks first, exactly as on the Startup page.
fn confirm_autostart_change<R: Runtime>(app: &AppHandle<R>, enabled: bool) -> AppResult<()> {
    if crate::autostart::is_enabled(app)? == enabled {
        return Ok(());
    }
    super::confirm::confirm(
        app,
        &super::inspect::startup_confirmation(&crate::autostart::entry_id(app), enabled),
    )
}

/// Read-modify-persist under the settings lock, then push the result into
/// running components before the lock is released.
pub fn save_settings<R: Runtime>(
    app: &AppHandle<R>,
    change: impl FnOnce(&mut Settings),
) -> AppResult<Settings> {
    let state = app.state::<AppState>();
    state.settings.update_with_autostart(
        change,
        || crate::autostart::is_enabled(app),
        |enabled| crate::autostart::set(app, enabled),
        |saved| apply_runtime(&state, saved),
    )
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
