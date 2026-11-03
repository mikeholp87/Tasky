//! The benchmark suite: shared, cross-platform algorithms and orchestration.
//! Only `platform` contains OS-specific code.
//!
//! See `docs/LINUX_BENCHMARK_IMPLEMENTATION.md` for the method definitions.

pub mod cancel;
pub mod coordinator;
pub mod cpu;
pub mod disk;
pub mod environment;
pub mod gpu;
pub mod history;
pub mod model;
pub mod scoring;
pub mod stats;

pub use cancel::CancelToken;
pub use model::*;

use crate::errors::AppError;

/// Why a benchmark run did not produce a result.
#[derive(Debug, thiserror::Error)]
pub enum BenchmarkError {
    #[error("The benchmark was cancelled.")]
    Cancelled,
    #[error("Another benchmark is currently running.")]
    AlreadyRunning,
    #[error("{0}")]
    Failed(String),
}

impl From<BenchmarkError> for AppError {
    fn from(e: BenchmarkError) -> Self {
        match e {
            BenchmarkError::AlreadyRunning => AppError::BenchmarkAlreadyRunning,
            BenchmarkError::Cancelled => AppError::BenchmarkFailed("it was cancelled".into()),
            BenchmarkError::Failed(m) => AppError::BenchmarkFailed(m),
        }
    }
}
