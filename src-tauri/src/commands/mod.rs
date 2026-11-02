//! Tauri command handlers. Each submodule groups commands by feature; handlers
//! validate input and delegate to the engine or platform layer.

pub mod control;
pub mod monitoring;
pub mod processes;
mod validate;
