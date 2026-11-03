//! Task Manager backend: application wiring only. Monitoring, platform and
//! command logic live in their own modules.

mod commands;
mod desktop;
mod logging;
mod settings;
mod state;

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use tauri::{Manager, RunEvent, WindowEvent};
use tauri_plugin_window_state::{StateFlags, WindowExt};
use tm_core::benchmarks::environment::SystemProbe;
use tm_core::benchmarks::history::HistoryStore;
use tm_core::benchmarks::mock::MockFactory;
use tm_core::benchmarks::network::inbox::NetworkInbox;
use tm_core::benchmarks::{
    BenchmarkCoordinator, HardwareFactory, SIMULATED_STEP, simulation_enabled,
};
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
            let logging = logging::init(
                paths.app_log_dir().ok().as_deref(),
                s.debug_logging,
                s.benchmark.debug_logging,
            );

            let engine = Arc::new(Engine::start(MonitoringState {
                paused: false,
                interval_ms: s.refresh_interval_ms,
            })?);
            let history_dir = paths
                .app_data_dir()
                .ok()
                .map(|d| d.join("benchmarks").join("results"));
            // Default storage-test folder: inside the app cache, on the app's volume.
            let disk_dir = paths.app_cache_dir().ok().map(|d| d.join("benchmark"));
            let network_inbox = Arc::new(NetworkInbox::default());
            let benchmarks = BenchmarkCoordinator::new(
                Arc::clone(&engine) as Arc<dyn SystemProbe>,
                Arc::new(HardwareFactory {
                    disk_dir: disk_dir.clone(),
                    network_inbox: Arc::clone(&network_inbox),
                }),
                Arc::new(MockFactory::new(SIMULATED_STEP)),
                Arc::new(HistoryStore::new(history_dir)),
                simulation_enabled(),
                disk_dir,
            );
            // Remove storage-test files abandoned by a crash, once the engine has
            // listed the mounted volumes.
            {
                let benchmarks = Arc::clone(&benchmarks);
                let engine = Arc::clone(&engine);
                let _ = std::thread::Builder::new()
                    .name("benchmark-sweep".into())
                    .spawn(move || {
                        for _ in 0..50 {
                            if engine.latest().is_some() {
                                break;
                            }
                            std::thread::sleep(Duration::from_millis(200));
                        }
                        benchmarks.sweep_abandoned_files();
                    });
            }
            let state = AppState {
                engine: Arc::clone(&engine),
                benchmarks,
                network_inbox,
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
            commands::benchmarks::get_benchmark_capabilities,
            commands::benchmarks::run_benchmark_preflight,
            commands::benchmarks::start_benchmark,
            commands::benchmarks::cancel_benchmark,
            commands::benchmarks::skip_benchmark_cooldown,
            commands::benchmarks::list_benchmark_history,
            commands::benchmarks::get_benchmark_result,
            commands::benchmarks::delete_benchmark_result,
            commands::benchmarks::report_network_progress,
            commands::benchmarks::submit_network_samples,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|handle, event| {
        if let RunEvent::Exit = event
            && let Some(state) = handle.try_state::<AppState>()
        {
            // Stop a running benchmark first so its cleanup (temporary files,
            // GPU resources) completes before the process exits.
            state.benchmarks.shutdown(Duration::from_secs(5));
            state.engine.shutdown();
        }
    });
}
