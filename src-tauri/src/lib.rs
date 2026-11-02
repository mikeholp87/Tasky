//! Task Manager backend: application wiring only. Monitoring, platform and
//! command logic live in their own modules.

mod commands;
mod logging;
mod state;

use std::sync::Arc;

use tauri::{Manager, RunEvent};

use crate::state::AppState;
use tm_core::models::MonitoringState;
use tm_core::monitoring::Engine;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    logging::init();

    let app = tauri::Builder::default()
        .setup(|app| {
            let engine = Engine::start(MonitoringState::default())?;
            app.manage(AppState {
                engine: Arc::new(engine),
            });
            Ok(())
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
