//! Centralised monitoring. The engine is the only component that queries the
//! operating system for metrics; everything else reads its snapshots.

mod collector;
mod cpu;
pub mod details;
mod disks;
pub mod engine;
pub mod history;
mod memory;
mod network;
mod process_history;
mod processes;
pub mod rates;

pub use engine::{Engine, TickSink};
