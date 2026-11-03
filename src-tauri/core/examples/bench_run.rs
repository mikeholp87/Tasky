//! Developer tool: runs a benchmark through the real coordinator (real
//! engine, real history file) and prints what the UI would receive.
//!
//! ```sh
//! cargo run -p tm-core --release --example bench_run -- quick
//! cargo run -p tm-core --release --example bench_run -- extended cancel-after=20
//! ```

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tm_core::benchmarks::coordinator::{Coordinator, ProgressSink};
use tm_core::benchmarks::{
    BenchmarkKind, BenchmarkProfile, BenchmarkResult, BenchmarkState, BenchmarkStatus,
};
use tm_core::models::MonitoringState;
use tm_core::monitoring::Engine;

#[derive(Default)]
struct Print {
    last_stage: Mutex<String>,
    updates: Mutex<u32>,
}
impl ProgressSink for Print {
    fn status(&self, s: &BenchmarkStatus) {
        *self.updates.lock().unwrap() += 1;
        let mut last = self.last_stage.lock().unwrap();
        if *last != s.stage {
            println!(
                "  [{:>5.1}s] {:?} {:>3.0}%  {}",
                s.elapsed_ms as f64 / 1000.0,
                s.state,
                s.fraction * 100.0,
                s.stage
            );
            *last = s.stage.clone();
        }
    }
    fn finished(&self, _: &BenchmarkResult) {}
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let profile = match args.get(1).map(String::as_str) {
        Some("quick") => BenchmarkProfile::Quick,
        Some("extended") => BenchmarkProfile::Extended,
        _ => BenchmarkProfile::Standard,
    };
    let kind = if args.iter().any(|a| a == "gpu") {
        BenchmarkKind::Gpu
    } else {
        BenchmarkKind::Cpu
    };
    let cancel_after: Option<u64> = args
        .iter()
        .find_map(|a| a.strip_prefix("cancel-after=")?.parse().ok());
    let dir = std::env::temp_dir().join(format!("tm-bench-run-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    let engine = Arc::new(Engine::start(MonitoringState::default()).expect("engine"));
    std::thread::sleep(Duration::from_millis(1500)); // let the engine take its first samples
    let coord = Coordinator::new(engine, Some(&dir));
    let sink = Arc::new(Print::default());
    let t0 = Instant::now();
    coord
        .start(kind, profile, None, sink.clone())
        .expect("starts");
    println!("started {profile:?}");
    loop {
        std::thread::sleep(Duration::from_millis(100));
        if let Some(secs) = cancel_after
            && t0.elapsed() >= Duration::from_secs(secs)
            && coord.status().state.is_active()
        {
            println!("  -- cancelling at {:.1}s", t0.elapsed().as_secs_f64());
            coord.cancel();
        }
        if !coord.status().state.is_active() {
            break;
        }
    }
    let wall = t0.elapsed().as_secs_f64();
    let status = coord.status();
    let updates = *sink.updates.lock().unwrap();
    println!(
        "\nfinal state {:?} after {wall:.1}s, {updates} progress updates ({:.1}/s), message {:?}",
        status.state,
        f64::from(updates) / wall,
        status.message
    );
    if status.state == BenchmarkState::Completed
        && let Some(r) = coord.history().first()
    {
        println!(
            "scores single {:?} multi {:?}  stability {:?}  affinity {}",
            r.scores.map(|s| s.single_core),
            r.scores.map(|s| s.multi_core),
            r.stability,
            r.affinity
        );
        println!(
            "gpu score {:?}  method {}  warnings {:?}",
            r.gpu_score, r.method.method_id, r.warnings
        );
        if let Some(g) = &r.gpu {
            println!(
                "adapter {} ({:?}, {}) timing {:?}",
                g.adapter.name, g.adapter.device_type, g.adapter.backend, g.timing_method
            );
            for w in &g.workloads {
                println!(
                    "  {:<24} {:>9.2} {:<7} cv {:>5.2}% validated {}",
                    w.label,
                    w.median,
                    w.unit,
                    w.cv_percent.unwrap_or(0.0),
                    w.validated
                );
            }
            for n in &g.notes {
                println!("  note: {n}");
            }
        }
        println!("env {:?}", r.environment);
        println!("warnings {:?}", r.warnings);
        if let Some(s) = &r.sustained {
            println!(
                "sustained: {} windows, peak {:.0} median {:.0} last-third {:.0} decline {:.1}%  note {:?}",
                s.windows.len(),
                s.peak_mops,
                s.median_mops,
                s.sustained_mops,
                s.decline_percent,
                s.note
            );
            let first = s.windows.first();
            let last = s.windows.last();
            println!(
                "  first window freq {:?} temp {:?}; last window freq {:?} temp {:?}",
                first.map(|w| w.frequency_mhz),
                first.map(|w| w.temperature_c),
                last.map(|w| w.frequency_mhz),
                last.map(|w| w.temperature_c)
            );
        }
        for w in &r.workloads {
            println!(
                "  {:?} {:<24} {:>10.1} {:<8} cv {:>5.2}%",
                w.mode,
                w.label,
                w.median,
                w.unit,
                w.cv_percent.unwrap_or(0.0)
            );
        }
    }
    let file = dir.join("benchmarks.json");
    println!(
        "history file: {} ({} results, {} bytes)",
        if file.exists() { "present" } else { "absent" },
        coord.history().len(),
        std::fs::metadata(&file).map_or(0, |m| m.len())
    );
    let _ = std::fs::remove_dir_all(&dir);
}
