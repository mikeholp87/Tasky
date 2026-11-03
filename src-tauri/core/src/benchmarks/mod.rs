//! Benchmarking subsystem. Methodology and architecture are documented in
//! `docs/BENCHMARKS.md` and `docs/BENCHMARK_IMPLEMENTATION_PLAN.md`.
//!
//! All workloads run in Rust on background threads; the UI only requests
//! runs through typed commands and receives throttled progress events.

pub mod cancellation;
pub mod confidence;
pub mod coordinator;
pub mod cpu;
pub mod disk;
pub mod environment;
pub mod gpu;
#[cfg(test)]
mod hardware_tests;
pub mod history;
pub mod mock;
pub mod models;
pub mod progress;
pub mod scoring;
pub mod statistics;
pub mod suite;
pub mod thermal;

use std::time::Duration;

pub use coordinator::BenchmarkCoordinator;
pub use scoring::SCORE_VERSION;

use crate::errors::{AppError, AppResult};
use models::{BenchmarkKind, BenchmarkRequest, CategoryResult, CategoryStatus, Detail};
use suite::{BenchmarkFactory, CategoryBenchmark, RunContext};

/// Stored-record layout version.
pub const SCHEMA_VERSION: u32 = 1;
/// Measurement methodology version. Bump whenever a workload, its sizing,
/// timing or statistics change; results of different versions are never
/// compared silently.
pub const METHOD_VERSION: &str = "1.0.0";

/// Hard cap on CPU worker threads, whatever the machine reports.
pub const MAX_CPU_WORKERS: u32 = 256;
pub const MIN_SUSTAINED_SECS: u32 = 30;
pub const MAX_SUSTAINED_SECS: u32 = 300;
/// Step length of simulated workloads.
pub const SIMULATED_STEP: Duration = Duration::from_millis(120);

/// Simulated runs are available in development builds, or when explicitly
/// enabled with `TM_BENCH_SIMULATED=1` (integration testing).
pub fn simulation_enabled() -> bool {
    cfg!(debug_assertions) || std::env::var("TM_BENCH_SIMULATED").is_ok_and(|v| v == "1")
}

/// Validates and normalizes an untrusted request: categories are
/// de-duplicated and put in execution order, and every number is bounded.
pub fn validate_request(
    mut request: BenchmarkRequest,
    logical_cores: u32,
    simulation_enabled: bool,
) -> AppResult<BenchmarkRequest> {
    let invalid = |m: &str| Err(AppError::InvalidBenchmarkConfiguration(m.to_owned()));
    if request.kinds.is_empty() {
        return invalid("Select at least one benchmark.");
    }
    if request.kinds.len() > BenchmarkKind::ORDER.len() * 2 {
        return invalid("Too many benchmarks requested.");
    }
    request.kinds = BenchmarkKind::ORDER
        .into_iter()
        .filter(|k| request.kinds.contains(k))
        .collect();
    if let Some(w) = request.cpu_workers {
        let max = logical_cores.clamp(1, MAX_CPU_WORKERS);
        if w == 0 || w > max {
            return invalid(&format!("CPU worker count must be between 1 and {max}."));
        }
    }
    if let Some(id) = &request.gpu_adapter_id
        && (id.is_empty() || id.len() > 128 || !id.bytes().all(|b| b.is_ascii_graphic()))
    {
        return invalid("Unknown GPU adapter.");
    }
    if let Some(mount) = &request.disk_mount_point
        && (mount.is_empty() || mount.len() > 4_096 || mount.contains('\0'))
    {
        return invalid("Invalid benchmark location.");
    }
    if let Some(secs) = request.sustained_secs
        && !(MIN_SUSTAINED_SECS..=MAX_SUSTAINED_SECS).contains(&secs)
    {
        return invalid(&format!(
            "Sustained test length must be between {MIN_SUSTAINED_SECS} and {MAX_SUSTAINED_SECS} seconds."
        ));
    }
    if request.simulate && !simulation_enabled {
        return invalid("Simulated benchmarks are not enabled in this build.");
    }
    Ok(request)
}

/// Real-hardware benchmarks. Categories not implemented on this platform
/// report `Unsupported` rather than inventing results.
#[derive(Default)]
pub struct HardwareFactory {
    /// App-owned benchmark folder on the app's own volume (the default
    /// storage-test location).
    pub disk_dir: Option<std::path::PathBuf>,
}

struct NotAvailable(BenchmarkKind);

impl CategoryBenchmark for NotAvailable {
    fn kind(&self) -> BenchmarkKind {
        self.0
    }

    fn run(&mut self, _ctx: &mut RunContext<'_, '_>) -> AppResult<CategoryResult> {
        let mut r = CategoryResult::empty(self.0, CategoryStatus::Unsupported);
        r.details.push(Detail::new(
            "reason",
            "Status",
            format!(
                "{} benchmark is not available in this build.",
                self.0.label()
            ),
        ));
        Ok(r)
    }
}

impl BenchmarkFactory for HardwareFactory {
    fn create(
        &self,
        kind: BenchmarkKind,
        _request: &BenchmarkRequest,
    ) -> AppResult<Box<dyn CategoryBenchmark>> {
        Ok(match kind {
            BenchmarkKind::Cpu => Box::new(cpu::CpuBenchmark),
            BenchmarkKind::Gpu => Box::new(gpu::GpuBenchmark),
            BenchmarkKind::Disk => Box::new(disk::DiskBenchmark {
                app_dir: self.disk_dir.clone(),
            }),
            _ => Box::new(NotAvailable(kind)),
        })
    }

    fn estimate_secs(&self, kind: BenchmarkKind, request: &BenchmarkRequest) -> u32 {
        match kind {
            BenchmarkKind::Cpu => cpu::estimate_secs(request),
            BenchmarkKind::Gpu => gpu::estimate_secs(request),
            BenchmarkKind::Disk => disk::estimate_secs(request),
            _ => 0,
        }
    }
}

/// A minimal valid request for unit tests.
#[cfg(test)]
pub fn coordinator_test_request() -> BenchmarkRequest {
    BenchmarkRequest {
        kinds: vec![BenchmarkKind::Cpu],
        profile: models::BenchmarkProfile::Quick,
        cpu_workers: None,
        gpu_adapter_id: None,
        disk_mount_point: None,
        disk_allow_write: false,
        disk_write_limit: models::DiskWriteLimit::default(),
        network_data_profile: models::NetworkDataProfile::DataSaver,
        sustained_secs: None,
        cooldown: false,
        simulate: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use models::{BenchmarkProfile, NetworkDataProfile};

    fn base() -> BenchmarkRequest {
        BenchmarkRequest {
            kinds: vec![
                BenchmarkKind::Network,
                BenchmarkKind::Cpu,
                BenchmarkKind::Cpu,
            ],
            profile: BenchmarkProfile::Standard,
            cpu_workers: None,
            gpu_adapter_id: None,
            disk_mount_point: None,
            disk_allow_write: false,
            disk_write_limit: models::DiskWriteLimit::default(),
            network_data_profile: NetworkDataProfile::Standard,
            sustained_secs: None,
            cooldown: true,
            simulate: false,
        }
    }

    #[test]
    fn normalizes_kind_order_and_duplicates() {
        let r = validate_request(base(), 8, false).unwrap();
        assert_eq!(r.kinds, vec![BenchmarkKind::Cpu, BenchmarkKind::Network]);
    }

    #[test]
    fn rejects_out_of_range_values() {
        let cases: Vec<BenchmarkRequest> = vec![
            BenchmarkRequest {
                kinds: vec![],
                ..base()
            },
            BenchmarkRequest {
                cpu_workers: Some(0),
                ..base()
            },
            BenchmarkRequest {
                cpu_workers: Some(9),
                ..base()
            },
            BenchmarkRequest {
                gpu_adapter_id: Some("a b".into()),
                ..base()
            },
            BenchmarkRequest {
                gpu_adapter_id: Some("x".repeat(200)),
                ..base()
            },
            BenchmarkRequest {
                disk_mount_point: Some(String::new()),
                ..base()
            },
            BenchmarkRequest {
                sustained_secs: Some(5),
                ..base()
            },
            BenchmarkRequest {
                sustained_secs: Some(10_000),
                ..base()
            },
            BenchmarkRequest {
                simulate: true,
                ..base()
            },
        ];
        for req in cases {
            let err = validate_request(req.clone(), 8, false).unwrap_err();
            assert_eq!(
                err.kind(),
                crate::errors::ErrorKind::InvalidBenchmarkConfiguration,
                "{req:?}"
            );
        }
        assert!(
            validate_request(
                BenchmarkRequest {
                    cpu_workers: Some(8),
                    ..base()
                },
                8,
                false
            )
            .is_ok()
        );
        assert!(
            validate_request(
                BenchmarkRequest {
                    simulate: true,
                    ..base()
                },
                8,
                true
            )
            .is_ok()
        );
    }

    #[test]
    fn request_round_trips_through_json() {
        let json = serde_json::to_string(&base()).unwrap();
        assert!(json.contains("\"networkDataProfile\":\"standard\""));
        let back: BenchmarkRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back, base());
    }
}
