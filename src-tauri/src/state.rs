use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use tm_core::monitoring::Engine;

use crate::desktop::WindowVisibility;
use crate::logging::Logging;
use crate::settings::SettingsStore;

/// Application state managed by Tauri and shared by all commands.
pub struct AppState {
    /// Shared so blocking work can run off the async executor.
    pub engine: Arc<Engine>,
    pub settings: SettingsStore,
    pub logging: Logging,
    pub visibility: Arc<WindowVisibility>,
    pub notifications_enabled: Arc<AtomicBool>,
}
