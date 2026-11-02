//! Task Manager backend: application wiring only. Monitoring, platform and
//! command logic live in their own modules.

mod commands;
mod desktop;
mod logging;
mod settings;
mod state;

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use tauri::{Manager, RunEvent, WindowEvent};
use tauri_plugin_window_state::{StateFlags, WindowExt};
use tm_core::models::MonitoringState;
use tm_core::monitoring::Engine;

use crate::desktop::{DesktopSink, MAIN_WINDOW, WindowVisibility, hide_main_window};
use crate::settings::SettingsStore;
use crate::state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        // Must be registered first: a second launch focuses the running
        // instance instead of starting another engine and window.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            desktop::show_main_window(app);
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        // Restored manually below, only when "Remember window position" is on.
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .skip_initial_state(MAIN_WINDOW)
                .build(),
        )
        .setup(|app| {
            let paths = app.path();
            let settings = SettingsStore::load(paths.app_config_dir().ok().as_deref());
            let s = settings.get();
            let logging = logging::init(paths.app_log_dir().ok().as_deref(), s.debug_logging);

            let engine = Arc::new(Engine::start(MonitoringState {
                paused: false,
                interval_ms: s.refresh_interval_ms,
            })?);
            let state = AppState {
                engine: Arc::clone(&engine),
                settings,
                logging,
                visibility: Arc::new(WindowVisibility::default()),
                notifications_enabled: Arc::new(AtomicBool::new(s.notifications_enabled)),
            };
            commands::settings::apply_runtime(&state, &s);
            let notifications = Arc::clone(&state.notifications_enabled);
            app.manage(state);

            let tray = desktop::build_tray(app.handle())?;
            app.manage(tray);
            engine.subscribe(
                "desktop".into(),
                Arc::new(DesktopSink::new(app.handle().clone(), notifications)),
            );

            if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
                if s.remember_window {
                    let _ = window.restore_state(StateFlags::all());
                }
                if !s.launch_minimized {
                    desktop::show_main_window(app.handle());
                } else if !s.minimize_to_tray {
                    // Without a tray to come back from, start minimized in the taskbar.
                    let _ = window.show();
                    let _ = window.minimize();
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != MAIN_WINDOW {
                return;
            }
            let app = window.app_handle();
            let Some(state) = app.try_state::<AppState>() else {
                return;
            };
            match event {
                WindowEvent::CloseRequested { api, .. }
                    if state.settings.get().minimize_to_tray =>
                {
                    api.prevent_close();
                    hide_main_window(app);
                }
                WindowEvent::Resized(_) | WindowEvent::Focused(_) => {
                    let visible = window.is_visible().unwrap_or(true)
                        && !window.is_minimized().unwrap_or(false);
                    let was = state.visibility.get();
                    state.visibility.set(visible);
                    if visible && !was {
                        let _ = state.engine.refresh_now();
                    }
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::monitoring::get_static_info,
            commands::monitoring::subscribe_metrics,
            commands::monitoring::get_history,
            commands::monitoring::get_monitoring_state,
            commands::monitoring::set_monitoring_state,
            commands::monitoring::refresh_now,
            commands::processes::get_process_details,
            commands::control::assess_process_action,
            commands::control::perform_process_action,
            commands::control::reveal_executable,
            commands::settings::get_settings,
            commands::settings::update_settings,
            commands::settings::reset_settings,
            commands::settings::get_recent_alerts,
            commands::export::export_processes,
            commands::export::export_system_summary,
            commands::export::export_diagnostics,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|handle, event| {
        if let RunEvent::Exit = event
            && let Some(state) = handle.try_state::<AppState>()
        {
            state.engine.shutdown();
        }
    });
}
