//! Centralised monitoring. The engine is the only component that queries the
//! operating system for metrics; everything else reads its snapshots.

mod collector;
mod cpu;
mod disks;
pub mod engine;
pub mod history;
mod memory;
mod network;
pub mod rates;

pub use engine::{Engine, TickSink};
