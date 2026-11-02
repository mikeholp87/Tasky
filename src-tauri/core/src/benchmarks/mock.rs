//! Deterministic mock benchmarks.
//!
//! Used by unit tests and by the opt-in "simulated" mode, which exercises
//! orchestration, progress, cancellation, persistence and the UI without
//! loading the hardware. Simulated results are always labelled as such and
//! carry a warning; they are never presented as measurements.

use std::collections::HashMap;
use std::time::Duration;

use super::models::{
    BenchmarkKind, BenchmarkPhase, BenchmarkRequest, BenchmarkWarning, CategoryResult,
    CategoryStatus, Detail, MetricUnit, WarningCode,
};
use super::suite::{BenchmarkFactory, CategoryBenchmark, RunContext, metric_from_samples};
use crate::errors::{AppError, AppResult};

#[derive(Debug, Clone)]
pub enum MockOutcome {
    Complete,
    Unsupported(String),
    Fail(String),
}

/// `(id, label, unit, higher_is_better, base value)` per kind. Ids match
/// the real benchmarks so scoring and the UI are exercised end to end.
fn metrics_for(
    kind: BenchmarkKind,
) -> &'static [(&'static str, &'static str, MetricUnit, bool, f64)] {
    match kind {
        BenchmarkKind::Cpu => &[
            (
                "cpu.single.integer",
                "Single-core integer",
                MetricUnit::Mops,
                true,
                2_100.0,
            ),
            (
                "cpu.multi.integer",
                "Multi-core integer",
                MetricUnit::Mops,
                true,
                15_400.0,
            ),
        ],
        BenchmarkKind::Gpu => &[
            (
                "gpu.compute.fma",
                "Compute (FMA)",
                MetricUnit::Gflops,
                true,
                4_800.0,
            ),
            (
                "gpu.memory.copy",
                "Memory copy",
                MetricUnit::GigabytesPerSec,
                true,
                180.0,
            ),
        ],
        BenchmarkKind::Disk => &[
            (
                "disk.seq.read",
                "Sequential read",
                MetricUnit::MegabytesPerSec,
                true,
                2_400.0,
            ),
            (
                "disk.seq.write",
                "Sequential write",
                MetricUnit::MegabytesPerSec,
                true,
                1_700.0,
            ),
        ],
        BenchmarkKind::Network => &[
            ("net.download", "Download", MetricUnit::Mbps, true, 250.0),
            (
                "net.latency.idle",
                "Idle latency",
                MetricUnit::Milliseconds,
                false,
                18.0,
            ),
        ],
    }
}

pub struct MockBenchmark {
    kind: BenchmarkKind,
    step: Duration,
    outcome: MockOutcome,
}

impl CategoryBenchmark for MockBenchmark {
    fn kind(&self) -> BenchmarkKind {
        self.kind
    }

    fn run(&mut self, ctx: &mut RunContext<'_, '_>) -> AppResult<CategoryResult> {
        ctx.reporter
            .phase(BenchmarkPhase::WarmingUp, "Warming up (simulated)");
        ctx.cancel.sleep(self.step * 2)?;
        match &self.outcome {
            MockOutcome::Complete => {}
            MockOutcome::Unsupported(msg) => {
                let mut r = CategoryResult::empty(self.kind, CategoryStatus::Unsupported);
                r.warnings
                    .push(BenchmarkWarning::new(WarningCode::Simulated, msg.clone()));
                return Ok(r);
            }
            MockOutcome::Fail(msg) => return Err(AppError::GpuInitializationFailed(msg.clone())),
        }
        ctx.reporter
            .phase(BenchmarkPhase::Running, "Running (simulated)");
        let iterations = ctx.request.profile.iterations();
        let defs = metrics_for(self.kind);
        let total = (defs.len() as u32 * iterations).max(1);
        let mut metrics = Vec::new();
        for (m, (id, label, unit, higher, base)) in defs.iter().enumerate() {
            ctx.reporter.subtest(*label);
            let mut samples = Vec::new();
            for i in 0..iterations {
                ctx.cancel.sleep(self.step)?;
                // Deterministic ±0.8 % pattern.
                let wobble = f64::from((i * 7 + m as u32) % 5) - 2.0;
                let value = base * (1.0 + 0.004 * wobble);
                samples.push(value);
                let done = m as u32 * iterations + i + 1;
                ctx.reporter.iteration(i + 1, iterations);
                ctx.reporter.measurement(*label, value, *unit);
                ctx.reporter.live(id, value, *unit);
                ctx.reporter
                    .progress(done as f32 / total as f32, format!("{label} (simulated)"));
            }
            metrics.push(metric_from_samples(id, label, *unit, *higher, samples));
        }
        let mut r = CategoryResult::empty(self.kind, CategoryStatus::Completed);
        r.metrics = metrics;
        r.details.push(Detail::new(
            "dataSource",
            "Data",
            "Simulated — no hardware was measured",
        ));
        r.warnings.push(BenchmarkWarning::new(
            WarningCode::Simulated,
            "Simulated result for testing. These values are not measurements.",
        ));
        Ok(r)
    }
}

pub struct MockFactory {
    pub step: Duration,
    pub outcomes: HashMap<BenchmarkKind, MockOutcome>,
}

impl MockFactory {
    pub fn new(step: Duration) -> Self {
        Self {
            step,
            outcomes: HashMap::new(),
        }
    }

    pub fn with(mut self, kind: BenchmarkKind, outcome: MockOutcome) -> Self {
        self.outcomes.insert(kind, outcome);
        self
    }
}

impl BenchmarkFactory for MockFactory {
    fn create(
        &self,
        kind: BenchmarkKind,
        _request: &BenchmarkRequest,
    ) -> AppResult<Box<dyn CategoryBenchmark>> {
        Ok(Box::new(MockBenchmark {
            kind,
            step: self.step,
            outcome: self
                .outcomes
                .get(&kind)
                .cloned()
                .unwrap_or(MockOutcome::Complete),
        }))
    }

    fn estimate_secs(&self, kind: BenchmarkKind, request: &BenchmarkRequest) -> u32 {
        let steps = metrics_for(kind).len() as u32 * request.profile.iterations() + 2;
        (steps * u32::try_from(self.step.as_millis()).unwrap_or(u32::MAX)).div_ceil(1_000)
    }
}
