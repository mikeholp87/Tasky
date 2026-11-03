//! Storage benchmark.
//!
//! 1. **Sequential write** — writes the temporary file in 1 MiB blocks, one
//!    segment per iteration, each followed by `sync_all` inside the timed
//!    region (data has been handed to the drive, not just the OS cache).
//!    With writes disabled the file is still created once, untimed.
//! 2. **Sequential read** — reads the file back in 1 MiB blocks.
//! 3. **Random read/write, 4 KiB** — QD1 (one thread, one outstanding
//!    request) and "8 threads × QD1" (eight threads with one request each;
//!    labelled as such, not as an asynchronous QD8). Per-operation latency
//!    gives p50/p95/p99; IOPS come from 0.25 s windows.
//!
//! Direct I/O (`O_DIRECT` / `FILE_FLAG_NO_BUFFERING`) is used when the
//! filesystem allows it; otherwise results are labelled "Buffered" and
//! flagged as possibly cache-affected. Writes are bounded by the configured
//! write limit and recorded.

pub mod file;
pub mod location;

use std::fs::File;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Barrier, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use file::{AlignedBuf, TempBenchFile};
use location::{DiskPlan, plan, prepare_dir, resolve};

use super::cancellation::CancelToken;
use super::cpu::kernels::Prng;
use super::models::{
    BenchmarkKind, BenchmarkPhase, BenchmarkProfile, BenchmarkRequest, BenchmarkWarning,
    CategoryResult, CategoryStatus, Detail, DiskLocation, MetricUnit, WarningCode,
};
use super::statistics::percentile_sorted;
use super::suite::{CategoryBenchmark, RunContext, metric_from_samples, metric_value};
use crate::errors::{AppError, AppResult};
use crate::platform::{read_exact_at, write_all_at};

const SEQ_BLOCK: usize = 1024 * 1024;
const RANDOM_BLOCK: usize = 4096;
const WINDOW: Duration = Duration::from_millis(250);
/// Latency samples kept per thread (enough for stable p99).
const MAX_LATENCIES_PER_THREAD: usize = 200_000;
pub const PARALLEL_THREADS: usize = 8;

pub const fn random_duration(profile: BenchmarkProfile) -> Duration {
    match profile {
        BenchmarkProfile::Quick => Duration::from_secs(2),
        BenchmarkProfile::Standard => Duration::from_secs(4),
        BenchmarkProfile::Extended => Duration::from_secs(8),
    }
}

/// Throughput in decimal MB/s.
pub fn mb_per_sec(bytes: u64, secs: f64) -> f64 {
    bytes as f64 / secs.max(1e-9) / 1e6
}

pub fn iops(ops: u64, secs: f64) -> f64 {
    ops as f64 / secs.max(1e-9)
}

/// p50/p95/p99 and mean of latencies (µs). p99 only with ≥ 100 samples.
pub fn latency_percentiles(latencies: &mut [f32]) -> Option<(f64, f64, Option<f64>, f64)> {
    if latencies.is_empty() {
        return None;
    }
    latencies.sort_by(f32::total_cmp);
    let sorted: Vec<f64> = latencies.iter().map(|v| f64::from(*v)).collect();
    let mean = sorted.iter().sum::<f64>() / sorted.len() as f64;
    let p99 = (sorted.len() >= 100).then(|| percentile_sorted(&sorted, 0.99));
    Some((
        percentile_sorted(&sorted, 0.50),
        percentile_sorted(&sorted, 0.95),
        p99,
        mean,
    ))
}

fn io_err(e: std::io::Error) -> AppError {
    AppError::SystemQueryFailed(format!("Disk I/O failed: {e}"))
}

/// Disk activity planned for a request (preflight and the run itself).
pub fn plan_for(
    disks: &[crate::models::DiskSnapshot],
    request: &BenchmarkRequest,
    app_dir: Option<&std::path::Path>,
) -> AppResult<(DiskLocation, std::path::PathBuf, DiskPlan)> {
    let (location, dir) = resolve(disks, request.disk_mount_point.as_deref(), app_dir)?;
    let plan = plan(
        request.profile,
        request.disk_allow_write,
        request.disk_write_limit,
        location.available_bytes,
        location.total_bytes,
    )?;
    Ok((location, dir, plan))
}

fn drive_kind(kind: crate::models::DiskKind) -> &'static str {
    match kind {
        crate::models::DiskKind::Ssd => "SSD",
        crate::models::DiskKind::Hdd => "HDD",
        crate::models::DiskKind::Unknown => "Unknown",
    }
}

pub fn estimate_secs(request: &BenchmarkRequest) -> u32 {
    let random = random_duration(request.profile).as_secs() as u32;
    let tests = if request.disk_allow_write { 4 } else { 2 };
    // Sequential passes depend on the drive; assume ~1 GB/s.
    let seq = (location::target_file_bytes(request.profile) / 1_000_000_000) as u32 * 2 + 2;
    random * tests + seq + 2
}

struct RandomResult {
    window_iops: Vec<f64>,
    latencies_us: Vec<f32>,
    ops: u64,
    elapsed: f64,
}

/// Random 4 KiB I/O on `handles.len()` threads (one outstanding request per
/// thread) for `duration`, or until `byte_budget` is written.
fn random_io(
    handles: Vec<File>,
    write: bool,
    blocks: u64,
    duration: Duration,
    byte_budget: u64,
    direct: bool,
    cancel: &CancelToken,
) -> AppResult<RandomResult> {
    let threads = handles.len().max(1);
    let barrier = Barrier::new(threads);
    let written = AtomicU64::new(0);
    let error: Mutex<Option<std::io::Error>> = Mutex::new(None);
    let windows = (duration.as_secs_f64() / WINDOW.as_secs_f64()).ceil() as usize;
    let results: Vec<(Vec<u64>, Vec<f32>, u64, f64)> = thread::scope(|s| {
        let joins: Vec<_> = handles
            .into_iter()
            .enumerate()
            .map(|(idx, file)| {
                let (barrier, written, error) = (&barrier, &written, &error);
                s.spawn(move || {
                    let mut rng = Prng::new(0xD15C_0000 + idx as u64);
                    let mut buf = AlignedBuf::new(RANDOM_BLOCK);
                    for chunk in buf.as_mut_slice().chunks_mut(8) {
                        chunk.copy_from_slice(&rng.next_u64().to_le_bytes()[..chunk.len()]);
                    }
                    let mut counts = vec![0u64; windows + 1];
                    let mut lat = Vec::with_capacity(MAX_LATENCIES_PER_THREAD.min(65_536));
                    let mut ops = 0u64;
                    barrier.wait();
                    let start = Instant::now();
                    loop {
                        let elapsed = start.elapsed();
                        if elapsed >= duration || cancel.is_cancelled() {
                            break;
                        }
                        if write
                            && written.fetch_add(RANDOM_BLOCK as u64, Ordering::Relaxed)
                                >= byte_budget
                        {
                            break;
                        }
                        let offset = (rng.next_u64() % blocks) * RANDOM_BLOCK as u64;
                        let t0 = Instant::now();
                        let r = if write {
                            write_all_at(&file, buf.as_slice(), offset)
                        } else {
                            read_exact_at(&file, buf.as_mut_slice(), offset)
                        };
                        if let Err(e) = r {
                            error
                                .lock()
                                .unwrap_or_else(PoisonError::into_inner)
                                .get_or_insert(e);
                            break;
                        }
                        let us = t0.elapsed().as_secs_f32() * 1e6;
                        if lat.len() < MAX_LATENCIES_PER_THREAD {
                            lat.push(us);
                        }
                        ops += 1;
                        let w = (elapsed.as_secs_f64() / WINDOW.as_secs_f64()) as usize;
                        if let Some(c) = counts.get_mut(w) {
                            *c += 1;
                        }
                    }
                    // Buffered writes may still sit in the OS cache: flush
                    // inside the measured time.
                    if write
                        && !direct
                        && let Err(e) = file.sync_all()
                    {
                        error
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .get_or_insert(e);
                    }
                    (counts, lat, ops, start.elapsed().as_secs_f64())
                })
            })
            .collect();
        joins
            .into_iter()
            .map(|j| {
                j.join()
                    .unwrap_or_else(|_| (Vec::new(), Vec::new(), 0, 0.0))
            })
            .collect()
    });
    if let Some(e) = error.into_inner().unwrap_or_else(PoisonError::into_inner) {
        return Err(io_err(e));
    }
    cancel.check()?;
    let elapsed = results.iter().map(|r| r.3).fold(0.0, f64::max);
    let ops: u64 = results.iter().map(|r| r.2).sum();
    let full_windows = ((elapsed / WINDOW.as_secs_f64()).floor() as usize).min(windows);
    let window_iops = (0..full_windows)
        .map(|w| {
            let n: u64 = results
                .iter()
                .map(|r| r.0.get(w).copied().unwrap_or(0))
                .sum();
            iops(n, WINDOW.as_secs_f64())
        })
        .collect();
    let latencies_us = results.into_iter().flat_map(|r| r.1).collect();
    Ok(RandomResult {
        window_iops,
        latencies_us,
        ops,
        elapsed,
    })
}

pub struct DiskBenchmark {
    pub app_dir: Option<std::path::PathBuf>,
}

impl CategoryBenchmark for DiskBenchmark {
    fn kind(&self) -> BenchmarkKind {
        BenchmarkKind::Disk
    }

    fn run(&mut self, rc: &mut RunContext<'_, '_>) -> AppResult<CategoryResult> {
        rc.reporter
            .phase(BenchmarkPhase::Preparing, "Checking the selected drive");
        // The volume list comes from the monitoring engine; right after start-up
        // its first snapshot may not exist yet.
        let deadline = Instant::now() + Duration::from_secs(5);
        let disks = loop {
            if let Some(tick) = rc.probe.latest() {
                break tick.disks.clone();
            }
            if Instant::now() >= deadline {
                break Vec::new();
            }
            rc.cancel.sleep(Duration::from_millis(100))?;
        };
        let (location, dir, plan) = plan_for(&disks, rc.request, self.app_dir.as_deref())?;
        let dir = prepare_dir(&dir, &location.mount_point)?;
        let profile = rc.request.profile;
        let iterations = profile.iterations();
        let allow_write = rc.request.disk_allow_write;
        let total_steps = if allow_write { 6.0 } else { 4.0 };
        let mut warnings = Vec::new();
        if let Some(r) = &plan.reduced {
            warnings.push(BenchmarkWarning::new(WarningCode::SizeReduced, r.clone()));
        }

        // RAII: the file is deleted on every exit path from here on.
        let temp = TempBenchFile::create(&dir).map_err(|e| match e.kind() {
            std::io::ErrorKind::PermissionDenied => AppError::DiskPermissionDenied(format!(
                "Task Manager cannot create files on {}.",
                location.mount_point
            )),
            _ => io_err(e),
        })?;
        let (mut handle, mut direct) = temp.open(true).map_err(io_err)?;

        let segment_bytes =
            (plan.file_bytes / u64::from(iterations)) / SEQ_BLOCK as u64 * SEQ_BLOCK as u64;
        let file_bytes = segment_bytes * u64::from(iterations);
        let mut buf = AlignedBuf::new(SEQ_BLOCK);
        let mut rng = Prng::new(0x5E9_0001);
        for chunk in buf.as_mut_slice().chunks_mut(8) {
            chunk.copy_from_slice(&rng.next_u64().to_le_bytes()[..chunk.len()]);
        }
        let mut bytes_written = 0u64;

        // --- Sequential write / file creation --------------------------------
        let label = if allow_write {
            "Sequential write"
        } else {
            "Preparing test file (not measured)"
        };
        rc.reporter.subtest(label);
        rc.reporter.phase(BenchmarkPhase::Running, label);
        let mut seq_write = Vec::new();
        for i in 0..iterations {
            let base = u64::from(i) * segment_bytes;
            let t0 = Instant::now();
            let mut offset = base;
            while offset < base + segment_bytes {
                rc.cancel.check()?;
                match write_all_at(&handle, buf.as_slice(), offset) {
                    Ok(()) => {}
                    // Some filesystems accept O_DIRECT at open but reject
                    // the I/O; fall back to buffered and label it.
                    Err(e)
                        if direct
                            && offset == 0
                            && e.kind() == std::io::ErrorKind::InvalidInput =>
                    {
                        let (h, d) = temp.open(false).map_err(io_err)?;
                        handle = h;
                        direct = d;
                        write_all_at(&handle, buf.as_slice(), offset).map_err(io_err)?;
                    }
                    Err(e) => return Err(io_err(e)),
                }
                offset += SEQ_BLOCK as u64;
                bytes_written += SEQ_BLOCK as u64;
            }
            handle.sync_all().map_err(io_err)?;
            let rate = mb_per_sec(segment_bytes, t0.elapsed().as_secs_f64());
            seq_write.push(rate);
            rc.reporter.iteration(i + 1, iterations);
            if allow_write {
                rc.reporter
                    .measurement(label, rate, MetricUnit::MegabytesPerSec);
                rc.reporter
                    .live("disk.seq.write", rate, MetricUnit::MegabytesPerSec);
            }
            rc.reporter
                .progress((i + 1) as f32 / iterations as f32 / total_steps, label);
        }

        // --- Sequential read ------------------------------------------------
        let label = "Sequential read";
        rc.reporter.subtest(label);
        let mut seq_read = Vec::new();
        for i in 0..iterations {
            let base = u64::from(i) * segment_bytes;
            let t0 = Instant::now();
            let mut offset = base;
            while offset < base + segment_bytes {
                rc.cancel.check()?;
                read_exact_at(&handle, buf.as_mut_slice(), offset).map_err(io_err)?;
                offset += SEQ_BLOCK as u64;
            }
            let rate = mb_per_sec(segment_bytes, t0.elapsed().as_secs_f64());
            seq_read.push(rate);
            rc.reporter.iteration(i + 1, iterations);
            rc.reporter
                .measurement(label, rate, MetricUnit::MegabytesPerSec);
            rc.reporter
                .live("disk.seq.read", rate, MetricUnit::MegabytesPerSec);
            rc.reporter.progress(
                (1.0 + (i + 1) as f32 / iterations as f32) / total_steps,
                label,
            );
        }
        drop(handle);

        // --- Random I/O -----------------------------------------------------
        let blocks = file_bytes / RANDOM_BLOCK as u64;
        let duration = random_duration(profile);
        let mut metrics = Vec::new();
        if allow_write {
            metrics.push(metric_from_samples(
                "disk.seq.write",
                "Sequential write",
                MetricUnit::MegabytesPerSec,
                true,
                seq_write,
            ));
        }
        metrics.push(metric_from_samples(
            "disk.seq.read",
            "Sequential read",
            MetricUnit::MegabytesPerSec,
            true,
            seq_read,
        ));

        let mut tests: Vec<(bool, usize)> = vec![(false, 1), (false, PARALLEL_THREADS)];
        if allow_write {
            tests.extend([(true, 1), (true, PARALLEL_THREADS)]);
        }
        let mut random_written = 0u64;
        let mut first_write = true;
        for (step, (write, threads)) in tests.into_iter().enumerate() {
            let op = if write { "write" } else { "read" };
            let suffix = if threads == 1 { "qd1" } else { "t8" };
            let label = if threads == 1 {
                format!("Random 4K {op}, QD1")
            } else {
                format!("Random 4K {op}, {threads} threads × QD1")
            };
            rc.reporter.subtest(&label);
            rc.reporter.phase(BenchmarkPhase::Running, label.clone());
            let mut handles = Vec::with_capacity(threads);
            for _ in 0..threads {
                handles.push(temp.open(direct).map_err(io_err)?.0);
            }
            // The first write test gets half the random-write budget, the second the rest.
            let budget = if first_write {
                plan.random_write_bytes / 2
            } else {
                plan.random_write_bytes.saturating_sub(random_written)
            };
            let result = random_io(handles, write, blocks, duration, budget, direct, rc.cancel)?;
            if write {
                let wrote = result.ops * RANDOM_BLOCK as u64;
                bytes_written += wrote;
                random_written += wrote;
                first_write = false;
            }
            let overall = iops(result.ops, result.elapsed);
            for v in &result.window_iops {
                rc.reporter
                    .live(&format!("disk.rand.{op}.{suffix}"), *v, MetricUnit::Iops);
            }
            rc.reporter.measurement(&label, overall, MetricUnit::Iops);
            rc.reporter
                .progress((2.0 + step as f32 + 1.0) / total_steps, label.clone());
            let id = format!("disk.rand.{op}.{suffix}");
            // A fast drive can exhaust the write budget before one full window;
            // then the whole test is the only sample.
            let samples = if result.window_iops.is_empty() {
                vec![overall]
            } else {
                result.window_iops
            };
            let mut m = metric_from_samples(&id, &label, MetricUnit::Iops, true, samples);
            m.note = Some(format!(
                "{:.0} MB/s; {} operations in {:.1} s",
                mb_per_sec(result.ops * RANDOM_BLOCK as u64, result.elapsed),
                result.ops,
                result.elapsed
            ));
            metrics.push(m);
            let mut lat = result.latencies_us;
            if let Some((p50, p95, p99, mean)) = latency_percentiles(&mut lat) {
                let base = format!("disk.latency.{op}.{suffix}");
                let short = if threads == 1 { "QD1" } else { "8 threads" };
                metrics.push(metric_value(
                    &format!("{base}.mean"),
                    &format!("Random {op} latency, {short} (average)"),
                    MetricUnit::Microseconds,
                    false,
                    mean,
                ));
                metrics.push(metric_value(
                    &format!("{base}.p50"),
                    &format!("Random {op} latency, {short} (p50)"),
                    MetricUnit::Microseconds,
                    false,
                    p50,
                ));
                metrics.push(metric_value(
                    &format!("{base}.p95"),
                    &format!("Random {op} latency, {short} (p95)"),
                    MetricUnit::Microseconds,
                    false,
                    p95,
                ));
                if let Some(p99) = p99 {
                    metrics.push(metric_value(
                        &format!("{base}.p99"),
                        &format!("Random {op} latency, {short} (p99)"),
                        MetricUnit::Microseconds,
                        false,
                        p99,
                    ));
                }
            }
        }
        metrics.push(metric_value(
            "disk.bytes_written",
            "Data written by this benchmark",
            MetricUnit::Bytes,
            false,
            bytes_written as f64,
        ));
        drop(temp);

        let total_ram = rc.probe.static_info().total_memory_bytes;
        if !direct {
            warnings.push(BenchmarkWarning::new(
                WarningCode::BufferedIo,
                "I/O Mode: Buffered. This filesystem does not support direct I/O, so the \
                 operating system cache may affect the results.",
            ));
            if file_bytes < total_ram {
                warnings.push(BenchmarkWarning::new(
                    WarningCode::CacheAffected,
                    "The test file is smaller than system memory and buffered I/O was used: read \
                     results may reflect RAM caching rather than drive speed.",
                ));
            }
        }

        let mb = |b: u64| format!("{} MiB", b / (1024 * 1024));
        let mut details = vec![
            Detail::new(
                "hardware",
                "Drive",
                if location.name.trim().is_empty() {
                    format!(
                        "{} drive ({})",
                        drive_kind(location.kind),
                        location.mount_point
                    )
                } else {
                    format!("{} ({})", location.name.trim(), location.mount_point)
                },
            ),
            Detail::new("location", "Location", location.mount_point.clone()),
            Detail::new("fileSystem", "File system", location.file_system.clone()),
            Detail::new("driveKind", "Drive type", drive_kind(location.kind)),
            Detail::new(
                "ioMode",
                "I/O mode",
                if direct {
                    "Direct (operating system cache bypassed)"
                } else {
                    "Buffered (operating system cache may affect results)"
                },
            ),
            Detail::new("fileSize", "Test file size", mb(file_bytes)),
            Detail::new(
                "blockSizes",
                "Block sizes",
                "Sequential 1 MiB; random 4 KiB (4 KiB-aligned offsets)",
            ),
            Detail::new(
                "queueDepth",
                "Queue depth",
                format!(
                    "QD1 (1 thread, 1 outstanding request) and {PARALLEL_THREADS} threads × QD1 \
                     (synchronous I/O; not an asynchronous queue)"
                ),
            ),
            Detail::new(
                "flush",
                "Write completion",
                if direct {
                    "Write-through direct I/O; sequential segments end with a full flush (sync). \
                     Drives may still acknowledge writes from their own internal cache, so short \
                     write tests can exceed sustained flash speed."
                } else {
                    "Buffered writes; a full flush (sync) is included in each timed segment and \
                     at the end of random-write tests"
                },
            ),
            Detail::new(
                "bytesWritten",
                "Data written",
                format!("{:.2} GB", bytes_written as f64 / 1e9),
            ),
        ];
        if !allow_write {
            details.push(Detail::new(
                "writeTests",
                "Write tests",
                "Skipped (disabled in settings). Creating the test file still wrote it once.",
            ));
        }

        let mut result = CategoryResult::empty(BenchmarkKind::Disk, CategoryStatus::Completed);
        result.metrics = metrics;
        result.details = details;
        result.warnings = warnings;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn throughput_and_iops_math() {
        assert!((mb_per_sec(3_000_000_000, 1.0) - 3_000.0).abs() < 1e-9);
        assert!((mb_per_sec(500_000_000, 0.5) - 1_000.0).abs() < 1e-9);
        assert!((iops(120_000, 2.0) - 60_000.0).abs() < 1e-9);
        // 4 KiB at 10 000 IOPS = 40.96 MB/s.
        assert!((mb_per_sec(10_000 * 4096, 1.0) - 40.96).abs() < 1e-9);
    }

    #[test]
    fn latency_percentiles_need_enough_samples_for_p99() {
        let mut few: Vec<f32> = (1..=50).map(|v| v as f32).collect();
        let (p50, p95, p99, mean) = latency_percentiles(&mut few).unwrap();
        assert!((p50 - 25.5).abs() < 1e-6);
        assert!((p95 - 47.55).abs() < 1e-4);
        assert!(p99.is_none());
        assert!((mean - 25.5).abs() < 1e-6);
        let mut many: Vec<f32> = (1..=1000).rev().map(|v| v as f32).collect();
        let (_, _, p99, _) = latency_percentiles(&mut many).unwrap();
        assert!((p99.unwrap() - 990.01).abs() < 1e-2);
        assert!(latency_percentiles(&mut []).is_none());
    }

    #[test]
    fn random_io_runs_reads_and_bounded_writes_on_a_small_file() {
        let dir = std::env::temp_dir().join(format!("tm-bench-rand-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let temp = TempBenchFile::create(&dir).unwrap();
        let (h, direct) = temp.open(false).unwrap();
        let block = vec![7u8; 1024 * 1024];
        write_all_at(&h, &block, 0).unwrap();
        drop(h);
        let cancel = CancelToken::new(Duration::from_secs(60));
        let blocks = (1024 * 1024 / RANDOM_BLOCK) as u64;
        let handles = vec![temp.open(false).unwrap().0, temp.open(false).unwrap().0];
        let r = random_io(
            handles,
            false,
            blocks,
            Duration::from_millis(600),
            0,
            direct,
            &cancel,
        )
        .unwrap();
        assert!(r.ops > 0);
        assert!(!r.window_iops.is_empty());
        assert!(!r.latencies_us.is_empty());
        let budget = 64 * RANDOM_BLOCK as u64;
        let handles = vec![temp.open(false).unwrap().0];
        let w = random_io(
            handles,
            true,
            blocks,
            Duration::from_secs(5),
            budget,
            direct,
            &cancel,
        )
        .unwrap();
        assert!(w.ops <= 64, "write budget respected ({} ops)", w.ops);
        let path = temp.path().to_path_buf();
        drop(temp);
        assert!(!path.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
