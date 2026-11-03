//! Developer tool: runs the CPU benchmark in release mode and prints the
//! medians, spread and timings. Used to calibrate sizes and check
//! repeatability.
//!
//! ```sh
//! cargo run -p tm-core --release --example bench_cpu -- standard 3 pin
//! ```

use std::time::Instant;

use tm_core::benchmarks::cpu::runner::{CpuHooks, CpuRunConfig, Observer, run_cpu};
use tm_core::benchmarks::{BenchmarkProfile, BenchmarkState, CancelToken, LiveReading};
use tm_core::platform;

struct Hooks;
impl CpuHooks for Hooks {
    fn pin_current_thread(&self, slot: usize) -> bool {
        platform::pin_current_thread(slot)
    }
    fn frequency_mhz(&self) -> Option<f64> {
        None
    }
    fn temperature_c(&self) -> Option<f32> {
        None
    }
}
struct Quiet;
impl Observer for Quiet {
    fn stage(&self, _: BenchmarkState, _: &str, _: f32) {}
    fn live(&self, _: Option<LiveReading>) {}
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let profile = match args.get(1).map(String::as_str) {
        Some("quick") => BenchmarkProfile::Quick,
        Some("extended") => BenchmarkProfile::Extended,
        _ => BenchmarkProfile::Standard,
    };
    let runs: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);
    let pin = args.iter().any(|a| a == "pin");
    let cpus = std::thread::available_parallelism().map_or(1, usize::from);
    let mut cfg = CpuRunConfig::for_profile(profile, cpus);
    cfg.pin_single = pin;
    println!(
        "profile {profile:?}  cpus {cpus}  pin {pin}  debug_assertions {}",
        cfg!(debug_assertions)
    );

    let mut medians: std::collections::BTreeMap<String, Vec<f64>> = Default::default();
    for r in 0..runs {
        let t = Instant::now();
        let out = run_cpu(&cfg, &Hooks, &CancelToken::new(), &Quiet).expect("run");
        println!(
            "run {} took {:.1}s  ({})",
            r + 1,
            t.elapsed().as_secs_f64(),
            out.affinity
        );
        for w in &out.workloads {
            let key = format!("{:?} {}", w.mode, w.label);
            println!(
                "  {key:<44} median {:>10.1} {:<8} cv {:>5.2}%  iter time ~{:.2}s",
                w.median,
                w.unit,
                w.cv_percent.unwrap_or(0.0),
                // iteration seconds implied by the median throughput
                1.0 / w.median.max(1e-9)
            );
            medians.entry(key).or_default().push(w.median);
        }
        if let Some(s) = out.sustained {
            println!(
                "  sustained: peak {:.0} median {:.0} last-third {:.0} decline {:.1}%",
                s.peak_mops, s.median_mops, s.sustained_mops, s.decline_percent
            );
        }
    }
    if runs > 1 {
        println!("\nrun-to-run variation of the medians ({runs} runs):");
        for (k, v) in &medians {
            let cv = tm_core::benchmarks::stats::cv_percent(v).unwrap_or(0.0);
            println!(
                "  {k:<44} cv {cv:>5.2}%   min {:>10.1}  max {:>10.1}",
                v.iter().cloned().fold(f64::MAX, f64::min),
                v.iter().cloned().fold(f64::MIN, f64::max)
            );
        }
    }
}
