use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use tm_core::benchmarks::coordinator::Coordinator;
use tm_core::inspect::ThreadSampler;
use tm_core::monitoring::Engine;

use crate::desktop::WindowVisibility;
use crate::logging::Logging;
use crate::settings::SettingsStore;

/// Application state managed by Tauri and shared by all commands.
pub struct AppState {
    /// Shared so blocking work can run off the async executor.
    pub engine: Arc<Engine>,
    pub benchmarks: Arc<Coordinator>,
    /// Previous thread reading of the process being inspected (one at a time).
    pub threads: Arc<Mutex<ThreadSampler>>,
    pub settings: SettingsStore,
    pub logging: Logging,
    pub visibility: Arc<WindowVisibility>,
    pub notifications_enabled: Arc<AtomicBool>,
}
