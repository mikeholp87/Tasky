use crate::monitoring::Engine;

/// Application state managed by Tauri and shared by all commands.
pub struct AppState {
    pub engine: Engine,
}
