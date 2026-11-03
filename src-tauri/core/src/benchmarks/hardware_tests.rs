//! Opt-in hardware benchmarks. They load the real CPU/GPU/disk, so they are
//! `#[ignore]`d and never run in CI:
//!
//! ```text
//! cargo test --release -p tm-core bench_ -- --ignored --nocapture --test-threads=1
//! ```

use std::sync::Arc;
use std::time::{Duration, Instant};

use super::coordinator::BenchmarkCoordinator;
use super::history::HistoryStore;
use super::history::tests::temp_dir;
use super::mock::MockFactory;
use super::models::{BenchmarkEvent, BenchmarkKind, BenchmarkRecord, BenchmarkRequest};
use super::progress::tests::RecordingSink;
use super::{HardwareFactory, coordinator_test_request};
use crate::models::MonitoringState;
use crate::monitoring::Engine;

/// Runs `request` on real hardware through the coordinator and prints a
/// readable summary of every metric.
pub fn run(request: BenchmarkRequest) -> BenchmarkRecord {
    let engine = Arc::new(Engine::start(MonitoringState::default()).expect("engine"));
    let dir = temp_dir("hardware");
    let coordinator = BenchmarkCoordinator::new(
        engine.clone(),
        Arc::new(HardwareFactory {
            disk_dir: Some(dir.join("app-benchmark")),
            ..HardwareFactory::default()
        }),
        Arc::new(MockFactory::new(Duration::from_millis(1))),
        Arc::new(HistoryStore::new(Some(dir.clone()))),
        false,
        Some(dir.join("app-benchmark")),
    );
    let sink = Arc::new(RecordingSink::default());
    let id = coordinator.start(request, sink.clone()).expect("starts");
    let deadline = Instant::now() + Duration::from_secs(900);
    let finished = loop {
        let done = sink.events.lock().unwrap().iter().find_map(|e| match e {
            BenchmarkEvent::Finished { phase, error, .. } => Some((*phase, error.clone())),
            _ => None,
        });
        if let Some(f) = done {
            break f;
        }
        assert!(Instant::now() < deadline, "benchmark timed out");
        std::thread::sleep(Duration::from_millis(200));
    };
    println!(
        "finished: {finished:?}; progress messages: {}",
        sink.progress().len()
    );
    let record = coordinator.history().get(&id).expect("record saved");
    for c in &record.categories {
        println!(
            "== {:?} {:?} score={:?} confidence={:?}",
            c.kind, c.status, c.score, c.confidence
        );
        for m in &c.metrics {
            println!(
                "  {:28} {:>12.2} {:?}  cv={:>5.2}%  score={:?}",
                m.id,
                m.value,
                m.unit,
                m.summary.as_ref().map_or(0.0, |s| s.cv_percent),
                m.score.map(f64::round)
            );
        }
        for d in &c.details {
            println!("  {}: {}", d.label, d.value);
        }
        for w in &c.warnings {
            println!("  warning: {}", w.message);
        }
        if let Some(e) = &c.error {
            println!("  error: {}", e.message);
        }
    }
    engine.shutdown();
    let _ = std::fs::remove_dir_all(dir);
    record
}

fn request(kind: BenchmarkKind) -> BenchmarkRequest {
    BenchmarkRequest {
        kinds: vec![kind],
        ..coordinator_test_request()
    }
}

#[test]
#[ignore = "hardware benchmark; run explicitly"]
fn bench_cpu_quick() {
    let record = run(request(BenchmarkKind::Cpu));
    assert!(record.categories[0].score.is_some());
}

#[test]
#[ignore = "hardware benchmark; run explicitly"]
fn bench_gpu_quick() {
    let record = run(request(BenchmarkKind::Gpu));
    let gpu = &record.categories[0];
    // Either measured, or honestly unsupported on machines without a GPU.
    assert!(gpu.error.is_none(), "{:?}", gpu.error);
}

#[test]
#[ignore = "hardware benchmark; run explicitly"]
fn bench_list_gpu_adapters() {
    for a in super::gpu::list_adapters() {
        println!("{a:?}");
    }
}

/// Writes ~0.6 GB of temporary data to the system drive (Quick profile).
#[test]
#[ignore = "hardware benchmark; run explicitly"]
fn bench_disk_quick() {
    let record = run(BenchmarkRequest {
        disk_allow_write: true,
        ..request(BenchmarkKind::Disk)
    });
    let disk = &record.categories[0];
    assert!(disk.error.is_none(), "{:?}", disk.error);
    assert!(disk.metric("disk.seq.read").is_some());
    assert!(disk.metric("disk.rand.write.t8").is_some());
}
