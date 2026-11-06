//! Settings and alert commands.

use std::sync::atomic::Ordering;

use tauri::{AppHandle, Runtime, State};
use tauri_plugin_autostart::ManagerExt;
use tm_core::errors::{AppError, AppResult};
use tm_core::models::{AlertEvent, MonitoringState, Settings};
use tm_core::monitoring::engine::MAX_HISTORY_POINTS;

use crate::state::AppState;

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings.get()
}

/// Validates, applies and persists new settings. Side effects that can fail
/// (autostart registration) run before saving, so a failure leaves both the
/// system and the stored settings unchanged.
#[tauri::command]
pub fn update_settings<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    settings: Settings,
) -> AppResult<Settings> {
    settings.validate()?;
    let previous = state.settings.get();
    let saved = save_with_autostart(
        &previous,
        settings,
        |enabled| set_autostart(&app, enabled),
        |next| state.settings.save(next),
    )?;
    apply_runtime(&state, &saved);
    crate::desktop::update_pause_label(&app, state.engine.state().paused);
    Ok(saved)
}

#[tauri::command]
pub fn reset_settings<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> AppResult<Settings> {
    let defaults = Settings::default();
    let previous = state.settings.get();
    let saved = save_with_autostart(
        &previous,
        defaults,
        |enabled| set_autostart(&app, enabled),
        |next| state.settings.save(next),
    )?;
    apply_runtime(&state, &saved);
    crate::desktop::update_pause_label(&app, state.engine.state().paused);
    Ok(saved)
}

fn save_with_autostart(
    previous: &Settings,
    next: Settings,
    set_autostart: impl Fn(bool) -> AppResult<()>,
    save: impl FnOnce(Settings) -> AppResult<Settings>,
) -> AppResult<Settings> {
    let changed = previous.start_with_os != next.start_with_os;
    if changed {
        set_autostart(next.start_with_os)?;
    }
    match save(next) {
        Ok(saved) => Ok(saved),
        Err(error) if changed => {
            if let Err(rollback) = set_autostart(previous.start_with_os) {
                return Err(AppError::SystemQueryFailed(format!(
                    "Settings were not saved: {error}. Restoring the previous startup setting also failed: {rollback}"
                )));
            }
            Err(error)
        }
        Err(error) => Err(error),
    }
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
    state
        .logging
        .set_debug(s.debug_logging, s.benchmark.debug_logging);
    state.benchmarks.set_save_history(s.benchmark.save_history);
}

/// Points needed to cover the configured history length at the configured
/// interval, bounded by the engine's maximum.
pub fn history_points(s: &Settings) -> usize {
    let per_minute = 60_000 / s.refresh_interval_ms.max(1) as usize;
    (s.history_minutes as usize * per_minute).min(MAX_HISTORY_POINTS)
}

fn set_autostart<R: Runtime>(app: &AppHandle<R>, enabled: bool) -> AppResult<()> {
    let manager = app.autolaunch();
    let result = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };
    result.map_err(|e| {
        AppError::PermissionDenied(format!(
            "Could not {} starting with the operating system: {e}",
            if enabled { "enable" } else { "disable" }
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_save_restores_previous_autostart_registration() {
        let previous = Settings::default();
        let next = Settings {
            start_with_os: true,
            ..previous.clone()
        };
        let changes = std::cell::RefCell::new(Vec::new());
        let result = save_with_autostart(
            &previous,
            next,
            |enabled| {
                changes.borrow_mut().push(enabled);
                Ok(())
            },
            |_| Err(AppError::SystemQueryFailed("disk unavailable".into())),
        );
        assert!(result.is_err());
        assert_eq!(*changes.borrow(), vec![true, false]);
    }

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
