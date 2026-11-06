//! Tauri command handlers. Each submodule groups commands by feature; handlers
//! validate input and delegate to the engine or platform layer.

pub mod benchmarks;
mod blocking;
mod confirm;
pub mod control;
pub mod export;
pub mod inspect;
pub mod monitoring;
pub mod processes;
pub mod settings;
mod validate;
