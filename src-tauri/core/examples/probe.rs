//! Developer probe: runs the real monitoring engine for a few seconds and
//! prints what it reports plus per-cycle timing (avg / p95 / max).
//!
//! ```sh
//! cargo run -p tm-core --release --example probe -- 20
//! ```
//!
//! Prints no command lines (they can contain secrets).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tm_core::models::{MetricsTick, MonitoringState};
use tm_core::monitoring::{Engine, TickSink};

#[derive(Default)]
struct Capture {
    ticks: Mutex<Vec<(f64, usize, Arc<MetricsTick>)>>,
}

impl TickSink for Capture {
    fn deliver(&self, tick: &MetricsTick, json: Option<&str>) -> bool {
        let bytes = json.map_or(0, str::len);
        if let Ok(mut t) = self.ticks.lock() {
            t.push((tick.engine.poll_duration_ms, bytes, Arc::new(tick.clone())));
        }
        true
    }
}

fn stats(mut v: Vec<f64>) -> String {
    if v.is_empty() {
        return "n/a".into();
    }
    v.sort_by(f64::total_cmp);
    let avg = v.iter().sum::<f64>() / v.len() as f64;
    let p95 = v[((v.len() as f64 * 0.95).ceil() as usize).saturating_sub(1)];
    let max = v[v.len() - 1];
    format!("avg {avg:.2}  p95 {p95:.2}  max {max:.2}")
}

fn main() {
    let cycles: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(10);
    let engine = Engine::start(MonitoringState {
        paused: false,
        interval_ms: 500,
    })
    .expect("engine starts");
    let capture = Arc::new(Capture::default());
    engine.subscribe("probe".into(), capture.clone());
    std::thread::sleep(Duration::from_millis(500 * cycles as u64 + 250));
    engine.shutdown();

    let info = engine.static_info();
    println!("{info:#?}");
    let ticks = capture.ticks.lock().expect("lock").clone();
    let Some((_, _, last)) = ticks.last() else {
        println!("no ticks");
        return;
    };
    println!(
        "\ncpu {:.1}%  freq {:?} MHz  load {:?}",
        last.cpu.usage_percent, last.cpu.frequency_mhz, last.cpu.load_average
    );
    println!("memory {:#?}", last.memory);
    println!("\ndisks:");
    for d in &last.disks {
        println!(
            "  {:<28} {:<22} {:<8} total {:>6} GiB  read {:?} B/s  active {:?}",
            d.name,
            d.mount_point,
            d.file_system,
            d.total_bytes >> 30,
            d.read_bytes_per_sec.map(|r| r as u64),
            d.active_percent.map(|a| a.round())
        );
    }
    println!("\nnetworks:");
    for n in &last.networks {
        println!(
            "  {:<16} {:?} primary={} rx {:>8.0} B/s tx {:>8.0} B/s ipv4 {:?}",
            n.name, n.kind, n.is_primary, n.rx_bytes_per_sec, n.tx_bytes_per_sec, n.ipv4
        );
    }
    println!("\ngpus {:#?}", last.gpus);
    println!("\ntemperatures:");
    for t in &last.temperatures {
        println!(
            "  {:<32} {:>5.1} °C  max {:?} crit {:?}",
            t.label, t.celsius, t.max_celsius, t.critical_celsius
        );
    }
    println!(
        "\nkernel threads {}",
        last.processes.iter().filter(|p| p.kernel_thread).count()
    );
    let mut states = std::collections::BTreeMap::new();
    for p in &last.processes {
        *states.entry(format!("{:?}", p.status)).or_insert(0) += 1;
    }
    println!("\nprocesses {}  states {states:?}", last.processes.len());
    println!(
        "unresolved users {}  missing threads {}",
        last.processes.iter().filter(|p| p.user.is_none()).count(),
        last.processes
            .iter()
            .filter(|p| p.threads.is_none())
            .count()
    );

    // Skip the first tick: it primes caches (exe, cmd, users).
    let steady: Vec<_> = ticks.iter().skip(1).collect();
    println!("\ncycles {}", steady.len());
    println!(
        "poll ms     {}",
        stats(steady.iter().map(|t| t.0).collect())
    );
    println!(
        "payload KiB {}",
        stats(steady.iter().map(|t| t.1 as f64 / 1024.0).collect())
    );
}
