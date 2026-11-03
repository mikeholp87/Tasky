//! Developer tool: lists graphics adapters and runs the GPU benchmark on one.
//!
//! ```sh
//! cargo run -p tm-core --release --example bench_gpu -- list
//! cargo run -p tm-core --release --example bench_gpu -- run standard 3
//! cargo run -p tm-core --release --example bench_gpu -- run quick 1 <adapter-id>
//! ```

use std::time::Instant;

use tm_core::benchmarks::cpu::runner::Observer;
use tm_core::benchmarks::gpu::GpuBackend;
use tm_core::benchmarks::gpu::runner::{GpuRunConfig, run_gpu};
use tm_core::benchmarks::gpu::wgpu_backend::WgpuBackend;
use tm_core::benchmarks::{
    BenchmarkProfile, BenchmarkState, CancelToken, GpuDeviceType, LiveReading,
};

struct Quiet;
impl Observer for Quiet {
    fn stage(&self, _: BenchmarkState, _: &str, _: f32) {}
    fn live(&self, _: Option<LiveReading>) {}
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let backend = WgpuBackend::new();
    let t = Instant::now();
    let adapters = backend.adapters().expect("enumerate");
    println!(
        "enumeration took {:.0} ms",
        t.elapsed().as_secs_f64() * 1000.0
    );
    for a in &adapters {
        println!(
            "  {} {:<34} {:?} {} | driver {} {} | ts {} | max buf {} MiB, binding {} MiB, wg {}{}",
            a.id,
            a.name,
            a.device_type,
            a.backend,
            a.driver,
            a.driver_info,
            a.supports_timestamps,
            a.max_buffer_bytes >> 20,
            a.max_storage_binding_bytes >> 20,
            a.max_workgroups_per_dimension,
            if a.recommended {
                "  <- recommended"
            } else {
                ""
            }
        );
    }
    if args.get(1).map(String::as_str) != Some("run") {
        return;
    }
    let profile = match args.get(2).map(String::as_str) {
        Some("quick") => BenchmarkProfile::Quick,
        Some("extended") => BenchmarkProfile::Extended,
        _ => BenchmarkProfile::Standard,
    };
    let runs: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1);
    let id = args.get(4).map(String::as_str);
    let mut medians: std::collections::BTreeMap<String, Vec<f64>> = Default::default();
    for r in 0..runs {
        let probe = adapters
            .iter()
            .find(|a| Some(a.id.as_str()) == id)
            .or_else(|| adapters.iter().find(|a| a.recommended))
            .expect("an adapter");
        let _ = probe;
        let t = Instant::now();
        let (info, mut exec, cfg) = backend
            .open(id, &|a| {
                GpuRunConfig::for_profile(profile, a.device_type == GpuDeviceType::Discrete)
            })
            .expect("open");
        let opened = t.elapsed().as_secs_f64();
        let t = Instant::now();
        let out = run_gpu(&mut *exec, &cfg, &CancelToken::new(), &Quiet).expect("run");
        println!(
            "\nrun {} on {} ({:?}) opened in {:.2}s, ran {:.1}s, timing {:?}",
            r + 1,
            info.name,
            info.device_type,
            opened,
            t.elapsed().as_secs_f64(),
            out.timing_method
        );
        for w in &out.workloads {
            println!(
                "  {:<24} {:>10.2} {:<7} cv {:>5.2}%  dispatch {:>6.1} ms  work {:.3e}  validated {}",
                w.label,
                w.median,
                w.unit,
                w.cv_percent.unwrap_or(0.0),
                w.dispatch_ms,
                w.work_per_dispatch,
                w.validated
            );
            medians.entry(w.label.clone()).or_default().push(w.median);
        }
        if let Some(s) = out.sustained {
            println!(
                "  sustained: peak {:.0} median {:.0} last-third {:.0} decline {:.1}%  note {:?}",
                s.peak_mops, s.median_mops, s.sustained_mops, s.decline_percent, s.note
            );
        }
    }
    if runs > 1 {
        println!("\nrun-to-run variation of the medians ({runs} runs):");
        for (k, v) in &medians {
            let cv = tm_core::benchmarks::stats::cv_percent(v).unwrap_or(0.0);
            println!(
                "  {k:<24} cv {cv:>5.2}%  min {:>9.2}  max {:>9.2}",
                v.iter().cloned().fold(f64::MAX, f64::min),
                v.iter().cloned().fold(f64::MIN, f64::max)
            );
        }
    }
}
