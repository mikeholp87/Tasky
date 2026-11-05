//! Runs the disk workloads against a real temporary file (Unix).
//!
//! Phases, in order: sequential write, verification, sequential read, random
//! reads at each queue depth, random writes at each queue depth, each
//! followed by verification. Queue depth N is N threads each issuing
//! synchronous I/O, so with direct I/O exactly N operations are outstanding at
//! the device. Everything read back is checked against what was written.

#![cfg(unix)]

use std::os::unix::fs::FileExt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Barrier, Mutex, PoisonError};
use std::time::{Duration, Instant};

use super::super::cancel::CancelToken;
use super::super::cpu::runner::Observer;
use super::super::model::{BenchmarkState, DiskPhaseKind, DiskPhaseResult, IoMode, LiveReading};
use super::super::{BenchmarkError, stats};
use super::aligned::AlignedBuf;
use super::file::{BenchFile, io_error};
use super::latency::LatencyLog;
use super::pattern::{
    RANDOM_WRITE_SALT, Rng, UNIT, fill_random, first_bad_header, first_bad_unit, init_units,
    pass_salt, stamp, stamp_units,
};
use super::plan::{DiskConfig, RAND_BLOCK, SEQ_BLOCK, plan_file_bytes};
use super::{DiskRequest, DiskRunOutput};

const BASE_SEED: u64 = 0x6469_736B_6265_6E63;
/// Blocks read back and fully compared after a write phase.
const VERIFY_SAMPLES: u64 = 256;

/// Raw outcome of one phase before it is summarised.
struct PhaseData {
    /// MB/s (decimal) per pass or segment.
    mbps: Vec<f64>,
    ops: u64,
    bytes: u64,
    seconds: f64,
    latency: LatencyLog,
}

struct Run<'a> {
    bf: &'a BenchFile,
    file_bytes: u64,
    /// The 4 KiB random block every unit is built from.
    base: Vec<u8>,
    cancel: &'a CancelToken,
    timeout: Duration,
    /// See `DiskConfig`: time limits that let slow drives finish.
    random_budget: Duration,
    seq_budget: Duration,
    observer: &'a dyn Observer,
    total_units: usize,
    done: AtomicUsize,
}

fn out_of_memory() -> BenchmarkError {
    BenchmarkError::Failed("Not enough memory for the disk test buffers.".into())
}

fn corrupt(offset: u64) -> BenchmarkError {
    BenchmarkError::Failed(format!(
        "The data read back from the drive does not match what was written (at offset {offset}), so the measurement was discarded."
    ))
}

impl Run<'_> {
    /// Stops the phase on cancellation or when it has run too long.
    fn check(&self, started: Instant) -> Result<(), BenchmarkError> {
        if self.cancel.is_cancelled() {
            return Err(BenchmarkError::Cancelled);
        }
        if started.elapsed() > self.timeout {
            return Err(BenchmarkError::Failed(
                "A disk operation took too long, so the test was stopped. The drive may be failing or busy."
                    .into(),
            ));
        }
        Ok(())
    }

    fn advance(&self, label: &str, live: Option<(f64, &str)>) {
        let done = self.done.fetch_add(1, Ordering::Relaxed) + 1;
        self.observer.stage(
            BenchmarkState::Running,
            label,
            (done as f32 / self.total_units as f32).min(1.0),
        );
        if let Some((value, unit)) = live {
            self.observer.live(Some(LiveReading {
                label: label.into(),
                value,
                unit: unit.into(),
            }));
        }
    }

    fn seq_blocks(&self) -> u64 {
        self.file_bytes / SEQ_BLOCK as u64
    }

    fn buffer(&self, len: usize) -> Result<AlignedBuf, BenchmarkError> {
        let mut b = AlignedBuf::new(len).ok_or_else(out_of_memory)?;
        init_units(b.as_mut_slice(), &self.base);
        Ok(b)
    }

    /// Whole-file sequential writes. The first pass creates the file's
    /// contents; later passes overwrite it. Each pass ends with a flush
    /// (`fdatasync`), which is inside the timing: this is durable throughput.
    fn seq_write(&self, passes: usize) -> Result<PhaseData, BenchmarkError> {
        let mut buf = self.buffer(SEQ_BLOCK)?;
        let blocks = self.seq_blocks();
        let mut log = LatencyLog::with_capacity(blocks as usize * passes);
        let (mut mbps, mut total_secs) = (Vec::new(), 0.0);
        let phase_start = Instant::now();
        for pass in 0..passes {
            // A slow drive gets fewer passes rather than a timeout.
            if pass >= 1 && phase_start.elapsed() > self.seq_budget {
                break;
            }
            let salt = pass_salt(pass as u32);
            let start = Instant::now();
            for b in 0..blocks {
                self.check(start)?;
                let off = b * SEQ_BLOCK as u64;
                stamp_units(buf.as_mut_slice(), off, salt);
                let t = Instant::now();
                self.bf
                    .file()
                    .write_all_at(buf.as_slice(), off)
                    .map_err(|e| io_error("write the test file", &e))?;
                log.record(t.elapsed());
            }
            self.bf.sync()?;
            let secs = start.elapsed().as_secs_f64();
            total_secs += secs;
            let rate = self.file_bytes as f64 / secs / 1e6;
            mbps.push(rate);
            self.advance("Sequential write", Some((rate, "MB/s")));
        }
        let done = mbps.len() as u64;
        Ok(PhaseData {
            mbps,
            ops: blocks * done,
            bytes: self.file_bytes * done,
            seconds: total_secs,
            latency: log,
        })
    }

    /// Whole-file sequential reads. Every unit's header is checked as it goes
    /// (cheap enough not to disturb the timing).
    fn seq_read(&self, passes: usize, salt: u64) -> Result<PhaseData, BenchmarkError> {
        let mut buf = AlignedBuf::new(SEQ_BLOCK).ok_or_else(out_of_memory)?;
        let blocks = self.seq_blocks();
        let mut log = LatencyLog::with_capacity(blocks as usize * passes);
        let (mut mbps, mut total_secs) = (Vec::new(), 0.0);
        let phase_start = Instant::now();
        for pass in 0..passes {
            if pass >= 1 && phase_start.elapsed() > self.seq_budget {
                break;
            }
            // Buffered reads must come from the drive, not this file's own
            // cached pages (only this file's; the system cache is untouched).
            self.bf.drop_own_cache();
            let start = Instant::now();
            for b in 0..blocks {
                self.check(start)?;
                let off = b * SEQ_BLOCK as u64;
                let t = Instant::now();
                self.bf
                    .file()
                    .read_exact_at(buf.as_mut_slice(), off)
                    .map_err(|e| io_error("read the test file", &e))?;
                log.record(t.elapsed());
                if let Some(bad) = first_bad_header(buf.as_slice(), off, salt) {
                    return Err(corrupt(bad));
                }
            }
            let secs = start.elapsed().as_secs_f64();
            total_secs += secs;
            let rate = self.file_bytes as f64 / secs / 1e6;
            mbps.push(rate);
            self.advance("Sequential read", Some((rate, "MB/s")));
        }
        let done = mbps.len() as u64;
        Ok(PhaseData {
            mbps,
            ops: blocks * done,
            bytes: self.file_bytes * done,
            seconds: total_secs,
            latency: log,
        })
    }

    /// Reads evenly spaced 4 KiB units back and compares them byte for byte
    /// with what was written. Not timed.
    fn verify_sample(&self, salt: u64) -> Result<(), BenchmarkError> {
        self.bf.drop_own_cache();
        let units = self.file_bytes / UNIT as u64;
        let n = VERIFY_SAMPLES.min(units);
        let mut buf = AlignedBuf::new(UNIT).ok_or_else(out_of_memory)?;
        for i in 0..n {
            let off = i * units / n * UNIT as u64;
            self.bf
                .file()
                .read_exact_at(&mut buf.as_mut_slice()[..UNIT], off)
                .map_err(|e| io_error("read the test file", &e))?;
            if let Some(bad) = first_bad_unit(&buf.as_slice()[..UNIT], &self.base, off, salt) {
                return Err(corrupt(bad));
            }
        }
        Ok(())
    }

    /// Random 4 KiB I/O at queue depth `qd`, in `segments` timed rounds. Each
    /// round is `qd` threads working at the same time between two barriers;
    /// a write round ends with a flush that is inside its timing. Returns the
    /// phase and (for writes) a sample of the offsets written.
    fn random(
        &self,
        write: bool,
        qd: u32,
        total_ops: u64,
        segments: usize,
        seq_salt: u64,
    ) -> Result<(PhaseData, Vec<u64>), BenchmarkError> {
        let qd = qd.max(1) as usize;
        let per_thread = (total_ops / segments as u64 / qd as u64).max(1);
        let blocks_total = self.file_bytes / RAND_BLOCK as u64;
        let label = if write { "Random write" } else { "Random read" };
        let barrier = Barrier::new(qd + 1);
        let stop = AtomicBool::new(false);
        let failure: Mutex<Option<BenchmarkError>> = Mutex::new(None);
        let logs: Mutex<Vec<LatencyLog>> = Mutex::new(Vec::new());
        let written: Mutex<Vec<u64>> = Mutex::new(Vec::new());
        let started = Instant::now();
        let mut round_secs = Vec::with_capacity(segments);
        // Each round may use its share of the phase budget; a fast drive
        // finishes its operations well inside it.
        let round_budget = self.random_budget / segments as u32;
        let round_ops: Vec<std::sync::atomic::AtomicU64> = (0..segments)
            .map(|_| std::sync::atomic::AtomicU64::new(0))
            .collect();

        let fail = |e: BenchmarkError| {
            failure
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get_or_insert(e);
            stop.store(true, Ordering::Relaxed);
        };

        std::thread::scope(|scope| {
            for t in 0..qd {
                let (barrier, stop, logs, written, fail, round_ops) =
                    (&barrier, &stop, &logs, &written, &fail, &round_ops);
                scope.spawn(move || {
                    let Some(mut buf) = AlignedBuf::new(RAND_BLOCK) else {
                        // Keep to the barrier protocol so nobody waits forever.
                        fail(out_of_memory());
                        for _ in 0..segments {
                            barrier.wait();
                            barrier.wait();
                        }
                        return;
                    };
                    init_units(buf.as_mut_slice(), &self.base);
                    let mut rng =
                        Rng(BASE_SEED ^ (t as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
                    let mut lat = LatencyLog::with_capacity((per_thread as usize) * segments);
                    let mut mine: Vec<u64> = Vec::new();
                    for ops in round_ops {
                        barrier.wait();
                        let round_end = Instant::now() + round_budget;
                        // A panic must not skip the barrier.
                        let r = catch_unwind(AssertUnwindSafe(|| -> Result<(), BenchmarkError> {
                            for i in 0..per_thread {
                                // At least one operation per round, then stop
                                // when the round's time is used up.
                                if stop.load(Ordering::Relaxed)
                                    || (i > 0 && Instant::now() >= round_end)
                                {
                                    break;
                                }
                                self.check(started)?;
                                let block = if write {
                                    // Threads own disjoint blocks, so every
                                    // written block's final content is known.
                                    rng.below(blocks_total / qd as u64) * qd as u64 + t as u64
                                } else {
                                    rng.below(blocks_total)
                                };
                                let off = block * RAND_BLOCK as u64;
                                let t0;
                                if write {
                                    stamp(buf.as_mut_slice(), off, RANDOM_WRITE_SALT);
                                    t0 = Instant::now();
                                    self.bf
                                        .file()
                                        .write_all_at(buf.as_slice(), off)
                                        .map_err(|e| io_error("write the test file", &e))?;
                                    if mine.len() < 128 {
                                        mine.push(off);
                                    }
                                } else {
                                    t0 = Instant::now();
                                    self.bf
                                        .file()
                                        .read_exact_at(buf.as_mut_slice(), off)
                                        .map_err(|e| io_error("read the test file", &e))?;
                                    if first_bad_header(buf.as_slice(), off, seq_salt).is_some() {
                                        return Err(corrupt(off));
                                    }
                                }
                                lat.record(t0.elapsed());
                                ops.fetch_add(1, Ordering::Relaxed);
                            }
                            Ok(())
                        }));
                        match r {
                            Ok(Ok(())) => {}
                            Ok(Err(e)) => fail(e),
                            Err(_) => fail(BenchmarkError::Failed(
                                "A disk worker stopped unexpectedly.".into(),
                            )),
                        }
                        barrier.wait();
                    }
                    logs.lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push(lat);
                    written
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .extend(mine);
                });
            }
            for ops in &round_ops {
                barrier.wait();
                let t0 = Instant::now();
                barrier.wait();
                if write
                    && !stop.load(Ordering::Relaxed)
                    && let Err(e) = self.bf.sync()
                {
                    fail(e);
                }
                let secs = t0.elapsed().as_secs_f64();
                round_secs.push(secs);
                let ops = ops.load(Ordering::Relaxed);
                let iops = ops as f64 / secs;
                self.advance(&format!("{label} · {qd}"), Some((iops, "IOPS")));
            }
        });

        if self.cancel.is_cancelled() {
            return Err(BenchmarkError::Cancelled);
        }
        if let Some(e) = failure
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            return Err(e);
        }
        // Operations actually completed in each round (fewer than planned on a
        // slow drive).
        let ops_per_round: Vec<u64> = round_ops
            .iter()
            .map(|o| o.load(Ordering::Relaxed))
            .collect();
        let total_ops: u64 = ops_per_round.iter().sum();
        let mbps: Vec<f64> = round_secs
            .iter()
            .zip(&ops_per_round)
            .map(|(s, ops)| *ops as f64 * RAND_BLOCK as f64 / s / 1e6)
            .collect();
        let mut log = LatencyLog::default();
        for l in logs.into_inner().unwrap_or_else(PoisonError::into_inner) {
            log.extend(l);
        }
        let sample = written.into_inner().unwrap_or_else(PoisonError::into_inner);
        Ok((
            PhaseData {
                mbps,
                ops: total_ops,
                bytes: total_ops * RAND_BLOCK as u64,
                seconds: round_secs.iter().sum(),
                latency: log,
            },
            sample,
        ))
    }

    /// Reads back sampled offsets from a random-write phase and compares them
    /// with what was written.
    fn verify_offsets(&self, offsets: &[u64]) -> Result<(), BenchmarkError> {
        self.bf.drop_own_cache();
        let mut buf = AlignedBuf::new(RAND_BLOCK).ok_or_else(out_of_memory)?;
        for off in offsets.iter().take(VERIFY_SAMPLES as usize) {
            self.bf
                .file()
                .read_exact_at(&mut buf.as_mut_slice()[..RAND_BLOCK], *off)
                .map_err(|e| io_error("read the test file", &e))?;
            if first_bad_unit(
                &buf.as_slice()[..RAND_BLOCK],
                &self.base,
                *off,
                RANDOM_WRITE_SALT,
            )
            .is_some()
            {
                return Err(corrupt(*off));
            }
        }
        Ok(())
    }
}

fn phase_result(
    kind: DiskPhaseKind,
    label: String,
    block: usize,
    concurrency: u32,
    queue_depth_exact: bool,
    data: PhaseData,
    note: Option<String>,
) -> Result<DiskPhaseResult, BenchmarkError> {
    let s = stats::summarize(&data.mbps)
        .filter(|s| s.median > 0.0)
        .ok_or_else(|| BenchmarkError::Failed("a measurement was not a valid number".into()))?;
    Ok(DiskPhaseResult {
        kind,
        label,
        block_bytes: block as u32,
        concurrency,
        queue_depth_exact,
        operations: data.ops,
        bytes: data.bytes,
        seconds: data.seconds,
        median_iops: s.median * 1e6 / block as f64,
        samples: data.mbps,
        median_mbps: s.median,
        mean_mbps: s.mean,
        min_mbps: s.min,
        max_mbps: s.max,
        cv_percent: s.cv_percent,
        outlier_indices: s.outliers.iter().map(|i| *i as u32).collect(),
        latency: data.latency.summary(),
        validated: true,
        note,
    })
}

/// "QD32" when each thread's operation is really outstanding at the device,
/// "32 threads" when buffered I/O makes that claim untrue.
fn depth_label(qd: u32, direct: bool) -> String {
    match (direct, qd) {
        (true, q) => format!("QD{q}"),
        (false, 1) => "1 thread".into(),
        (false, q) => format!("{q} threads"),
    }
}

pub fn run_disk(
    req: &DiskRequest,
    cancel: &CancelToken,
    observer: &dyn Observer,
) -> Result<DiskRunOutput, BenchmarkError> {
    if cancel.is_cancelled() {
        return Err(BenchmarkError::Cancelled);
    }
    let cfg: &DiskConfig = &req.config;
    let file_bytes = plan_file_bytes(cfg.file_bytes, cfg.min_file_bytes, req.free_bytes)
        .map_err(BenchmarkError::Failed)?;
    observer.stage(BenchmarkState::Preflight, "Creating the test file", 0.0);
    // Dropped (and the file removed) on every path out of this function.
    let bf = BenchFile::create(&req.parent, &req.run_id, req.want_direct)?;
    if cancel.is_cancelled() {
        return Err(BenchmarkError::Cancelled);
    }
    let direct = bf.mode() == IoMode::Direct;
    let mut base = vec![0u8; UNIT];
    fill_random(&mut base, BASE_SEED);

    let random_phases = cfg.read_concurrency.len() + cfg.write_concurrency.len();
    let total_units = cfg.seq_write_passes + 1 + cfg.seq_read_passes + random_phases * cfg.segments;
    let run = Run {
        bf: &bf,
        file_bytes,
        base,
        cancel,
        timeout: cfg.phase_timeout,
        random_budget: cfg.random_phase_budget,
        seq_budget: cfg.seq_phase_budget,
        observer,
        total_units,
        done: AtomicUsize::new(0),
    };

    let mut phases = Vec::new();
    let (mut written, mut read) = (0u64, 0u64);
    let seq_label = |what: &str| format!("Sequential {what} · 1 MiB");

    // Sequential write, then check that the data really is on the file.
    let w = run.seq_write(cfg.seq_write_passes)?;
    written += w.bytes;
    // The data on disk comes from the last pass that actually ran, which can
    // be fewer than planned when a slow drive used up its time budget.
    let last_salt = pass_salt(w.mbps.len().saturating_sub(1) as u32);
    run.verify_sample(last_salt)?;
    run.advance("Checking the written data", None);
    phases.push(phase_result(
        DiskPhaseKind::SequentialWrite,
        seq_label("write"),
        SEQ_BLOCK,
        1,
        direct,
        w,
        Some("Each pass ends with a flush to the drive, inside the timing.".into()),
    )?);

    let r = run.seq_read(cfg.seq_read_passes, last_salt)?;
    read += r.bytes;
    phases.push(phase_result(
        DiskPhaseKind::SequentialRead,
        seq_label("read"),
        SEQ_BLOCK,
        1,
        direct,
        r,
        None,
    )?);

    for &qd in &cfg.read_concurrency {
        // With buffered I/O the sequential passes just left this file's pages
        // in the cache. Drop them (only this file's, never the system's) so the
        // random reads start cold and are not mostly cache hits.
        bf.drop_own_cache();
        let (d, _) = run.random(false, qd, cfg.rand_read_ops, cfg.segments, last_salt)?;
        read += d.bytes;
        phases.push(phase_result(
            DiskPhaseKind::RandomRead,
            format!("Random read · 4 KiB · {}", depth_label(qd, direct)),
            RAND_BLOCK,
            qd,
            direct,
            d,
            None,
        )?);
    }
    for &qd in &cfg.write_concurrency {
        let (d, sample) = run.random(true, qd, cfg.rand_write_ops, cfg.segments, last_salt)?;
        written += d.bytes;
        run.verify_offsets(&sample)?;
        phases.push(phase_result(
            DiskPhaseKind::RandomWrite,
            format!("Random write · 4 KiB · {}", depth_label(qd, direct)),
            RAND_BLOCK,
            qd,
            direct,
            d,
            Some("Each round ends with a flush to the drive, inside the timing.".into()),
        )?);
    }

    Ok(DiskRunOutput {
        phases,
        io_mode: bf.mode(),
        io_mode_note: bf.mode_note().map(str::to_owned),
        file_bytes,
        bytes_written: written,
        bytes_read: read,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmarks::disk::file::DIR_NAME;
    use std::sync::atomic::AtomicUsize;

    struct Quiet(AtomicUsize);
    impl Observer for Quiet {
        fn stage(&self, _: BenchmarkState, _: &str, f: f32) {
            assert!((0.0..=1.0).contains(&f));
            self.0.fetch_add(1, Ordering::Relaxed);
        }
        fn live(&self, _: Option<LiveReading>) {}
    }

    fn request(dir: &std::path::Path, direct: bool) -> DiskRequest {
        DiskRequest {
            parent: dir.to_path_buf(),
            run_id: "disk-1-0".into(),
            config: DiskConfig::test(),
            free_bytes: 100 << 30,
            want_direct: direct,
        }
    }

    fn leftovers(dir: &std::path::Path) -> bool {
        dir.join(DIR_NAME).exists()
    }

    #[test]
    fn a_full_run_measures_validates_and_cleans_up_in_both_modes() {
        for direct in [false, true] {
            let d = tempfile::tempdir().unwrap();
            let obs = Quiet(AtomicUsize::new(0));
            let out =
                run_disk(&request(d.path(), direct), &CancelToken::new(), &obs).expect("runs");
            // Sequential write and read, plus 2 random reads and 2 random writes.
            assert_eq!(out.phases.len(), 6);
            assert!(
                out.phases
                    .iter()
                    .all(|p| p.validated && p.median_mbps > 0.0 && p.median_iops > 0.0)
            );
            assert_eq!(out.file_bytes, 4 << 20);
            // 2 sequential passes of 4 MiB, plus 2 random-write phases of 64 x 4 KiB.
            assert_eq!(out.bytes_written, 2 * (4 << 20) + 2 * 64 * 4096);
            assert_eq!(out.bytes_read, 2 * (4 << 20) + 2 * 64 * 4096);
            if !direct {
                assert_eq!(out.io_mode, IoMode::Buffered);
                assert!(out.phases.iter().all(|p| !p.queue_depth_exact));
                assert!(out.phases.iter().any(|p| p.label.contains("4 threads")));
            }
            if out.io_mode == IoMode::Direct {
                assert!(out.phases.iter().all(|p| p.queue_depth_exact));
                assert!(out.phases.iter().any(|p| p.label.contains("QD4")));
            }
            for p in &out.phases {
                let l = p.latency.as_ref().expect("latency recorded");
                assert!(
                    l.samples > 0 && l.p50_us <= l.p99_us && l.p99_us <= l.max_us,
                    "{}",
                    p.label
                );
                assert_eq!(p.samples.len(), 2);
            }
            assert!(obs.0.load(Ordering::Relaxed) > 6);
            assert!(!leftovers(d.path()), "no file or folder left behind");
        }
    }

    #[test]
    fn a_pre_cancelled_run_stops_at_once_and_leaves_nothing() {
        let d = tempfile::tempdir().unwrap();
        let c = CancelToken::new();
        c.cancel();
        let r = run_disk(&request(d.path(), false), &c, &Quiet(AtomicUsize::new(0)));
        assert!(matches!(r, Err(BenchmarkError::Cancelled)));
        assert!(!leftovers(d.path()));
    }

    #[test]
    fn cancelling_mid_run_stops_quickly_and_removes_the_file() {
        let d = tempfile::tempdir().unwrap();
        let mut req = request(d.path(), false);
        req.config.file_bytes = 64 << 20;
        req.config.seq_write_passes = 50; // far more than can finish
        let cancel = CancelToken::new();
        let c2 = cancel.clone();
        let t = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            c2.cancel();
        });
        let started = Instant::now();
        let r = run_disk(&req, &cancel, &Quiet(AtomicUsize::new(0)));
        t.join().unwrap();
        assert!(matches!(r, Err(BenchmarkError::Cancelled)), "{r:?}");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
        assert!(!leftovers(d.path()));
    }

    #[test]
    fn not_enough_free_space_is_refused_before_any_file_exists() {
        let d = tmp_dir_check();
        let mut req = request(d.path(), false);
        req.free_bytes = 1 << 30; // below the 2 GiB reserve
        let r = run_disk(&req, &CancelToken::new(), &Quiet(AtomicUsize::new(0)));
        let e = r.expect_err("refused");
        assert!(e.to_string().contains("Not enough free space"), "{e}");
        assert!(!leftovers(d.path()));
    }

    fn tmp_dir_check() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn a_stuck_phase_times_out_with_a_plain_message() {
        let d = tempfile::tempdir().unwrap();
        let mut req = request(d.path(), false);
        req.config.phase_timeout = Duration::ZERO;
        let r = run_disk(&req, &CancelToken::new(), &Quiet(AtomicUsize::new(0)));
        let e = r.expect_err("times out");
        assert!(e.to_string().contains("took too long"), "{e}");
        assert!(!leftovers(d.path()));
    }

    #[test]
    fn an_unwritable_location_is_a_clear_error() {
        let r = run_disk(
            &request(std::path::Path::new("/proc/definitely-not-writable"), false),
            &CancelToken::new(),
            &Quiet(AtomicUsize::new(0)),
        );
        assert!(r.is_err());
    }

    /// Writes data, then damages one block on disk, and checks the
    /// verification notices (the check must be able to fail).
    #[test]
    fn verification_catches_damaged_data() {
        let d = tempfile::tempdir().unwrap();
        let bf = BenchFile::create(d.path(), "disk-9-0", false).unwrap();
        let mut base = vec![0u8; UNIT];
        fill_random(&mut base, BASE_SEED);
        let cancel = CancelToken::new();
        let obs = Quiet(AtomicUsize::new(0));
        let run = Run {
            bf: &bf,
            file_bytes: 4 << 20,
            base,
            cancel: &cancel,
            timeout: Duration::from_secs(30),
            random_budget: Duration::from_secs(30),
            seq_budget: Duration::from_secs(30),
            observer: &obs,
            total_units: 100,
            done: AtomicUsize::new(0),
        };
        run.seq_write(1).unwrap();
        run.verify_sample(pass_salt(0))
            .expect("intact data verifies");
        // Damage one unit that the sample covers (offset 0).
        bf.file().write_all_at(&[0u8; 64], 100).unwrap();
        let e = run
            .verify_sample(pass_salt(0))
            .expect_err("damage is found");
        assert!(e.to_string().contains("does not match"), "{e}");
        // And wrong-salt data is rejected too (stale data from another pass).
        assert!(run.verify_sample(pass_salt(1)).is_err());
    }

    #[test]
    fn a_slow_drive_is_measured_with_fewer_operations_instead_of_timing_out() {
        let d = tempfile::tempdir().unwrap();
        let mut req = request(d.path(), false);
        // Far more random operations than fit in the time budget.
        req.config.rand_read_ops = 500_000_000;
        req.config.rand_write_ops = 500_000_000;
        req.config.read_concurrency = vec![1];
        req.config.write_concurrency = vec![1];
        req.config.random_phase_budget = Duration::from_millis(400);
        let started = Instant::now();
        let out =
            run_disk(&req, &CancelToken::new(), &Quiet(AtomicUsize::new(0))).expect("finishes");
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "{:?}",
            started.elapsed()
        );
        for p in out.phases.iter().filter(|p| p.block_bytes == 4096) {
            assert!(
                p.operations > 0 && p.operations < 500_000_000,
                "{} ops",
                p.operations
            );
            assert!(p.validated && p.median_iops > 0.0);
            assert_eq!(p.samples.len(), 2, "every segment still produced a sample");
        }
    }

    #[test]
    fn extra_sequential_passes_are_skipped_once_the_budget_is_spent() {
        let d = tempfile::tempdir().unwrap();
        let mut req = request(d.path(), false);
        req.config.seq_write_passes = 50;
        req.config.seq_read_passes = 50;
        req.config.seq_phase_budget = Duration::ZERO;
        let out =
            run_disk(&req, &CancelToken::new(), &Quiet(AtomicUsize::new(0))).expect("finishes");
        let seq: Vec<_> = out
            .phases
            .iter()
            .filter(|p| p.block_bytes == 1 << 20)
            .collect();
        // One pass is always done; no more once the budget is spent.
        assert!(
            seq.iter().all(|p| p.samples.len() == 1),
            "{:?}",
            seq.iter().map(|p| p.samples.len()).collect::<Vec<_>>()
        );
        assert_eq!(out.bytes_written, (4 << 20) + 2 * 64 * 4096);
    }

    #[test]
    fn queue_depth_is_only_claimed_when_it_is_true() {
        assert_eq!(depth_label(32, true), "QD32");
        assert_eq!(depth_label(32, false), "32 threads");
        assert_eq!(depth_label(1, false), "1 thread");
        assert_eq!(depth_label(1, true), "QD1");
    }
}
