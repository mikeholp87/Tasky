//! Task Manager core: the monitoring engine, data models and the platform
//! abstraction layer. It has no UI or Tauri dependency, so it can be tested
//! (and type-checked for other operating systems) on its own.

pub mod errors;
pub mod models;
pub mod monitoring;
pub mod platform;
pub mod process_control;
