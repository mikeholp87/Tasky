//! Developer tool: runs the internet benchmark against the real provider.
//! Sends and receives generated test data (the profile's data limits apply).
//!
//! ```sh
//! cargo run -p tm-core --release --example bench_net -- quick
//! ```

use std::time::Instant;

use tm_core::benchmarks::cpu::runner::Observer;
use tm_core::benchmarks::internet::provider::Cloudflare;
use tm_core::benchmarks::internet::route::{describe_path, read_default_route};
use tm_core::benchmarks::internet::{InternetConfig, run_internet};
use tm_core::benchmarks::{BenchmarkProfile, BenchmarkState, CancelToken, LiveReading};
use tm_core::models::InterfaceKind;

struct Quiet;
impl Observer for Quiet {
    fn stage(&self, _: BenchmarkState, _: &str, _: f32) {}
    fn live(&self, _: Option<LiveReading>) {}
}

fn main() {
    let profile = match std::env::args().nth(1).as_deref() {
        Some("extended") => BenchmarkProfile::Extended,
        Some("standard") => BenchmarkProfile::Standard,
        _ => BenchmarkProfile::Quick,
    };
    let path = describe_path(read_default_route(), &|_| InterfaceKind::Unknown, &[]);
    let t = Instant::now();
    let out = run_internet(
        &Cloudflare::new(),
        &InternetConfig::for_profile(profile),
        path,
        &CancelToken::new(),
        &Quiet,
    );
    match out {
        Ok(r) => println!("{:#?}\ntook {:.1}s", r, t.elapsed().as_secs_f64()),
        Err(e) => println!("FAILED: {e}\ntook {:.1}s", t.elapsed().as_secs_f64()),
    }
}
