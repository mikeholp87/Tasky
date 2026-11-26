// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // WebKitGTK's DMABUF renderer can freeze the UI after a long minimize on
    // Wayland (KDE/Intel and others). Fall back to the shared-memory path
    // unless the user already chose a value. Must run before GTK/WebKit start.
    // See https://v2.tauri.app/develop/debug/linux-graphics/
    #[cfg(target_os = "linux")]
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        // SAFETY: no other threads have been spawned yet.
        unsafe { std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1") };
    }

    taskmanager_lib::run()
}
