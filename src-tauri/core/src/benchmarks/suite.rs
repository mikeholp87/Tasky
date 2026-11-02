//! The contract between the coordinator and category benchmarks.

use super::cancellation::CancelToken;
use super::environment::SystemProbe;
use super::models::{BenchmarkKind, BenchmarkRequest, CategoryResult, Metric, MetricUnit};
use super::progress::Reporter;
use super::statistics::{outlier_indices, summarize};
use crate::errors::AppResult;

/// Everything a category benchmark may use while it runs.
pub struct RunContext<'r, 'a> {
    pub request: &'r BenchmarkRequest,
    pub cancel: &'r CancelToken,
    pub reporter: &'r mut Reporter<'a>,
    pub probe: &'r dyn SystemProbe,
}

/// One category (CPU, GPU, disk or network). Implementations must:
/// * call `ctx.cancel.check()` at least every ~100 ms of work;
/// * release every resource on all paths (RAII), including cancellation;
/// * return `Err(BenchmarkCancelled)` (from `check`) when cancelled, an
///   `Unsupported` result when the hardware cannot run the test, and other
///   errors only for genuine failures.
pub trait CategoryBenchmark: Send {
    fn kind(&self) -> BenchmarkKind;
    fn run(&mut self, ctx: &mut RunContext<'_, '_>) -> AppResult<CategoryResult>;
}

/// Creates category benchmarks; real hardware and mocks are interchangeable.
pub trait BenchmarkFactory: Send + Sync {
    fn create(
        &self,
        kind: BenchmarkKind,
        request: &BenchmarkRequest,
    ) -> AppResult<Box<dyn CategoryBenchmark>>;

    /// Rough duration for the pre-run summary.
    fn estimate_secs(&self, kind: BenchmarkKind, request: &BenchmarkRequest) -> u32;
}

/// Builds a metric from raw per-iteration samples: headline value = median,
/// with full summary and flagged outliers. Raw samples are kept as-is.
pub fn metric_from_samples(
    id: &str,
    label: &str,
    unit: MetricUnit,
    higher_is_better: bool,
    samples: Vec<f64>,
) -> Metric {
    let summary = summarize(&samples);
    Metric {
        id: id.to_owned(),
        label: label.to_owned(),
        unit,
        higher_is_better,
        value: summary.as_ref().map_or(f64::NAN, |s| s.median),
        outliers: outlier_indices(&samples),
        summary,
        samples,
        score: None,
        note: None,
    }
}

/// A derived single value (e.g. scaling factor) with no sample spread.
pub fn metric_value(
    id: &str,
    label: &str,
    unit: MetricUnit,
    higher_is_better: bool,
    value: f64,
) -> Metric {
    Metric {
        id: id.to_owned(),
        label: label.to_owned(),
        unit,
        higher_is_better,
        value,
        summary: None,
        samples: vec![value],
        outliers: Vec::new(),
        score: None,
        note: None,
    }
}
