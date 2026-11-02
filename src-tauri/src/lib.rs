//! Task Manager backend: application wiring only. Monitoring, platform and
//! command logic live in their own modules.

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
