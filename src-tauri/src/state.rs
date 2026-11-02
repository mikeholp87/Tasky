use std::sync::Arc;

use tm_core::monitoring::Engine;

/// Application state managed by Tauri and shared by all commands.
pub struct AppState {
    /// Shared so blocking work can run off the async executor.
    pub engine: Arc<Engine>,
}
