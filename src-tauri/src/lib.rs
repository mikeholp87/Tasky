//! Task Manager backend: application wiring only. Monitoring, platform and
//! command logic live in their own modules.

mod commands;
mod errors;
mod logging;
mod models;
mod monitoring;
mod platform;
mod state;

use tauri::{Manager, RunEvent};

use crate::models::MonitoringState;
use crate::monitoring::Engine;
use crate::state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    logging::init();

    let app = tauri::Builder::default()
        .setup(|app| {
            let engine = Engine::start(MonitoringState::default())?;
            app.manage(AppState { engine });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::monitoring::get_static_info,
            commands::monitoring::subscribe_metrics,
            commands::monitoring::get_history,
            commands::monitoring::get_monitoring_state,
            commands::monitoring::set_monitoring_state,
            commands::monitoring::refresh_now,
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
