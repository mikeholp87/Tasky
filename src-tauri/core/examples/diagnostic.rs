//! Opt-in, anonymized monitoring measurement. No benchmark workloads or network.
//! cargo run --release -p tm-core --example diagnostic -- 30
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tm_core::models::{MetricsTick, MonitoringState};
use tm_core::monitoring::{Engine, TickSink};

struct Sample {
    poll_ms: f64,
    serialization_ms: f64,
    payload_bytes: usize,
    processes: u32,
    rss_bytes: u64,
}

#[derive(Default)]
struct Samples(Mutex<Vec<Sample>>);

impl TickSink for Samples {
    fn wants_json(&self) -> bool {
        false
    }

    fn deliver(&self, tick: &MetricsTick, _: Option<&str>) -> bool {
        let began = Instant::now();
        let json = serde_json::to_vec(tick).expect("finite metrics serialize");
        let serialization_ms = began.elapsed().as_secs_f64() * 1_000.0;
        let own_memory = tick
            .processes
            .iter()
            .find(|p| p.pid == std::process::id())
            .map_or(0, |p| p.memory_bytes);
        self.0.lock().expect("measurement lock").push(Sample {
            poll_ms: tick.engine.poll_duration_ms,
            serialization_ms,
            payload_bytes: json.len(),
            processes: tick.engine.process_count,
            rss_bytes: own_memory,
        });
        true
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let count: usize = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "30".into())
        .parse()?;
    if !(5..=600).contains(&count) {
        return Err("sample count must be 5..=600".into());
    }
    let began = Instant::now();
    let engine = Engine::start(MonitoringState::default())?;
    let initialization_ms = began.elapsed().as_secs_f64() * 1_000.0;
    let samples = Arc::new(Samples::default());
    engine.subscribe("diagnostic".into(), samples.clone());
    let deadline = Instant::now() + Duration::from_secs(count as u64 * 3 + 10);
    while samples.0.lock().expect("measurement lock").len() < count {
        if Instant::now() >= deadline {
            engine.shutdown();
            return Err("sampling timed out".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    engine.shutdown();
    let samples = samples.0.lock().expect("measurement lock");
    let summary = |values: Vec<f64>| {
        let mut sorted = values;
        sorted.sort_by(f64::total_cmp);
        serde_json::json!({
            "mean": sorted.iter().sum::<f64>() / sorted.len() as f64,
            "p95": sorted[(sorted.len() as f64 * 0.95).ceil() as usize - 1],
            "min": sorted[0], "max": sorted[sorted.len() - 1],
        })
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
            "samples": samples.len(), "intervalMs": engine.state().interval_ms,
            "initializationMs": initialization_ms,
            "pollMs": summary(samples.iter().map(|s| s.poll_ms).collect()),
            "serializationMs": summary(samples.iter().map(|s| s.serialization_ms).collect()),
            "payloadBytes": summary(samples.iter().map(|s| s.payload_bytes as f64).collect()),
            "processCount": summary(samples.iter().map(|s| f64::from(s.processes)).collect()),
            "ownRssBytes": summary(samples.iter().map(|s| s.rss_bytes as f64).collect()),
        }))?
    );
    Ok(())
}
