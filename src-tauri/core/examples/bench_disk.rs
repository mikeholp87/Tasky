//! Developer tool: runs the disk benchmark on a real folder.
//!
//! ```sh
//! cargo run -p tm-core --release --example bench_disk -- standard 1 /home/me
//! cargo run -p tm-core --release --example bench_disk -- quick 3 /home/me buffered
//! ```

#[cfg(unix)]
mod unix {
    use std::path::PathBuf;
    use std::time::Instant;

    use tm_core::benchmarks::cpu::runner::Observer;
    use tm_core::benchmarks::disk::DiskRequest;
    use tm_core::benchmarks::disk::engine::run_disk;
    use tm_core::benchmarks::disk::file::{cleanup_stale, free_bytes};
    use tm_core::benchmarks::disk::plan::DiskConfig;
    use tm_core::benchmarks::{BenchmarkProfile, BenchmarkState, CancelToken, LiveReading};

    struct Quiet;
    impl Observer for Quiet {
        fn stage(&self, _: BenchmarkState, _: &str, _: f32) {}
        fn live(&self, _: Option<LiveReading>) {}
    }

    pub fn run() {
        let args: Vec<String> = std::env::args().collect();
        let profile = match args.get(1).map(String::as_str) {
            Some("quick") => BenchmarkProfile::Quick,
            Some("extended") => BenchmarkProfile::Extended,
            _ => BenchmarkProfile::Standard,
        };
        let runs: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);
        let parent = PathBuf::from(
            args.get(3)
                .cloned()
                .unwrap_or_else(|| std::env::var("HOME").unwrap()),
        );
        let direct = !args.iter().any(|a| a == "buffered");
        let free = free_bytes(&parent).expect("free space");
        println!(
            "target {} free {} GiB, direct requested {direct}",
            parent.display(),
            free >> 30
        );
        let mut medians: std::collections::BTreeMap<String, Vec<f64>> = Default::default();
        for r in 0..runs {
            let req = DiskRequest {
                parent: parent.clone(),
                run_id: format!("disk-example-{r}"),
                config: DiskConfig::for_profile(profile),
                free_bytes: free,
                want_direct: direct,
            };
            let t = Instant::now();
            let out = run_disk(&req, &CancelToken::new(), &Quiet).expect("run");
            println!(
                "\nrun {} took {:.1}s mode {:?} file {} MiB, written {} MiB, read {} MiB {}",
                r + 1,
                t.elapsed().as_secs_f64(),
                out.io_mode,
                out.file_bytes >> 20,
                out.bytes_written >> 20,
                out.bytes_read >> 20,
                out.io_mode_note.as_deref().unwrap_or("")
            );
            for p in &out.phases {
                let l = p.latency.as_ref().unwrap();
                println!(
                    "  {:<34} {:>9.1} MB/s {:>9.0} IOPS  cv {:>5.2}%  lat p50 {:>7.0} p95 {:>7.0} p99 {:>7.0} max {:>8.0} us  n={}",
                    p.label,
                    p.median_mbps,
                    p.median_iops,
                    p.cv_percent.unwrap_or(0.0),
                    l.p50_us,
                    l.p95_us,
                    l.p99_us,
                    l.max_us,
                    l.samples
                );
                medians
                    .entry(p.label.clone())
                    .or_default()
                    .push(if p.block_bytes >= 1 << 20 {
                        p.median_mbps
                    } else {
                        p.median_iops
                    });
            }
        }
        println!(
            "leftover files removed by stale cleanup: {}",
            cleanup_stale(&parent, std::time::Duration::ZERO)
        );
        if runs > 1 {
            println!("\nrun-to-run variation of the medians ({runs} runs):");
            for (k, v) in &medians {
                let cv = tm_core::benchmarks::stats::cv_percent(v).unwrap_or(0.0);
                println!(
                    "  {k:<34} cv {cv:>5.2}%  min {:>10.0}  max {:>10.0}",
                    v.iter().cloned().fold(f64::MAX, f64::min),
                    v.iter().cloned().fold(f64::MIN, f64::max)
                );
            }
        }
    }
}

#[cfg(unix)]
fn main() {
    unix::run();
}

#[cfg(not(unix))]
fn main() {
    eprintln!("The disk benchmark example is available on Unix only.");
}
