//! What a disk run does and how much space and wear it may use. Pure rules,
//! tested without touching a disk.

use std::time::Duration;

use super::super::model::BenchmarkProfile;

/// Sequential block size: large, like a copy of a big file.
pub const SEQ_BLOCK: usize = 1 << 20;
/// Random block size: the 4 KiB of typical small-file and database access.
pub const RAND_BLOCK: usize = 4096;
/// Below this the test is too small to mean anything.
pub const MIN_FILE_BYTES: u64 = 64 << 20;
/// Free space that is always left alone, whatever the percentage says.
pub const MIN_RESERVE: u64 = 2 << 30;

#[derive(Debug, Clone)]
pub struct DiskConfig {
    /// Size of the test file the plan would like.
    pub file_bytes: u64,
    /// Smallest file worth testing with; below it the run is refused.
    pub min_file_bytes: u64,
    /// Passes over the whole file. Writes are limited: they wear flash.
    pub seq_read_passes: usize,
    pub seq_write_passes: usize,
    /// Each random phase is split into this many timed segments, whose
    /// throughputs are the samples (so spread and outliers can be reported).
    pub segments: usize,
    /// Random operations per concurrency level, over all segments.
    pub rand_read_ops: u64,
    pub rand_write_ops: u64,
    /// Threads doing I/O at once. With direct I/O each thread's operation is
    /// outstanding at the device, so this is the queue depth.
    pub read_concurrency: Vec<u32>,
    pub write_concurrency: Vec<u32>,
    /// A phase that runs longer than this is abandoned.
    pub phase_timeout: Duration,
    /// A random phase stops adding rounds once its rounds have used this
    /// long, so a slow drive (a hard disk does about 100 random reads a
    /// second) is measured with fewer operations instead of timing out.
    pub random_phase_budget: Duration,
    /// Further sequential passes are skipped after this long, once at least
    /// one pass is done.
    pub seq_phase_budget: Duration,
}

impl DiskConfig {
    pub fn for_profile(profile: BenchmarkProfile) -> Self {
        let (file_mib, reads, writes, segments, rr, rw) = match profile {
            BenchmarkProfile::Quick => (256, 3, 1, 3, 12_000, 6_000),
            BenchmarkProfile::Standard => (1024, 5, 2, 5, 50_000, 20_000),
            BenchmarkProfile::Extended => (2048, 7, 3, 7, 150_000, 40_000),
        };
        Self {
            file_bytes: file_mib << 20,
            min_file_bytes: MIN_FILE_BYTES,
            seq_read_passes: reads,
            seq_write_passes: writes,
            segments,
            rand_read_ops: rr,
            rand_write_ops: rw,
            read_concurrency: vec![1, 8, 32],
            write_concurrency: vec![1, 32],
            phase_timeout: Duration::from_secs(180),
            random_phase_budget: Duration::from_secs(8),
            seq_phase_budget: Duration::from_secs(40),
        }
    }

    /// Tiny settings for tests.
    pub fn test() -> Self {
        Self {
            file_bytes: 4 << 20,
            min_file_bytes: 1 << 20,
            seq_read_passes: 2,
            seq_write_passes: 2,
            segments: 2,
            rand_read_ops: 64,
            rand_write_ops: 64,
            read_concurrency: vec![1, 4],
            write_concurrency: vec![1, 4],
            phase_timeout: Duration::from_secs(30),
            random_phase_budget: Duration::from_secs(30),
            seq_phase_budget: Duration::from_secs(30),
        }
    }
}

/// Free space to leave untouched: the larger of a fixed amount and a tenth of
/// what is free, so a nearly full disk is never filled.
pub fn reserve_bytes(free: u64) -> u64 {
    MIN_RESERVE.max(free / 10)
}

/// The file size to use: what the profile wants, cut down to fit, in whole
/// mebibytes. An error says why when there is not enough room.
pub fn plan_file_bytes(desired: u64, min: u64, free: u64) -> Result<u64, String> {
    let allowed = free.saturating_sub(reserve_bytes(free));
    let size = desired.min(allowed) / (1 << 20) * (1 << 20);
    if size < min {
        return Err(format!(
            "Not enough free space for the test: it needs at least {} MiB beyond the {} MiB kept free, and this location has {} MiB free.",
            min >> 20,
            reserve_bytes(free) >> 20,
            free >> 20
        ));
    }
    Ok(size)
}

/// Total bytes the run will write to the drive.
pub fn estimated_bytes_written(cfg: &DiskConfig, file_bytes: u64) -> u64 {
    file_bytes * cfg.seq_write_passes as u64
        + cfg.rand_write_ops * RAND_BLOCK as u64 * cfg.write_concurrency.len() as u64
}

/// Rough run time on a fast NVMe drive (measured: Quick 1 s, Standard 4 s).
/// Slower drives take longer, up to the time budgets in [`DiskConfig`]; only a
/// guide for the text beside the Run button.
pub fn estimated_seconds(profile: BenchmarkProfile) -> u32 {
    match profile {
        BenchmarkProfile::Quick => 3,
        BenchmarkProfile::Standard => 8,
        BenchmarkProfile::Extended => 25,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1 << 30;

    #[test]
    fn the_reserve_is_the_safer_of_a_fixed_and_a_percentage_amount() {
        assert_eq!(
            reserve_bytes(5 * GIB),
            2 * GIB,
            "fixed floor on a small disk"
        );
        assert_eq!(reserve_bytes(100 * GIB), 10 * GIB, "a tenth on a large one");
        assert_eq!(reserve_bytes(0), 2 * GIB);
    }

    #[test]
    fn the_file_never_eats_into_the_reserve() {
        // Plenty of room: the profile's size is used.
        assert_eq!(plan_file_bytes(GIB, MIN_FILE_BYTES, 500 * GIB), Ok(GIB));
        // Tight: cut down so the reserve survives.
        let size = plan_file_bytes(2 * GIB, MIN_FILE_BYTES, 3 * GIB).unwrap();
        assert!(size <= GIB, "{size}");
        assert!(3 * GIB - size >= reserve_bytes(3 * GIB));
        assert_eq!(size % (1 << 20), 0, "whole mebibytes");
    }

    #[test]
    fn too_little_room_is_a_clear_error_not_a_tiny_meaningless_test() {
        let e = plan_file_bytes(GIB, MIN_FILE_BYTES, 2 * GIB + 10 * (1 << 20)).unwrap_err();
        assert!(e.contains("Not enough free space"), "{e}");
        assert!(plan_file_bytes(GIB, MIN_FILE_BYTES, 0).is_err());
        assert!(
            plan_file_bytes(GIB, MIN_FILE_BYTES, GIB).is_err(),
            "below the reserve entirely"
        );
        // Just enough.
        assert!(plan_file_bytes(GIB, MIN_FILE_BYTES, 2 * GIB + 70 * (1 << 20)).is_ok());
        // A small test file is fine when the configuration says so.
        assert_eq!(plan_file_bytes(4 << 20, 1 << 20, 100 * GIB), Ok(4 << 20));
    }

    #[test]
    fn the_estimate_counts_the_sequential_passes_and_random_writes() {
        let c = DiskConfig::for_profile(BenchmarkProfile::Standard);
        let est = estimated_bytes_written(&c, c.file_bytes);
        let expect = c.file_bytes * 2 + 20_000 * 4096 * 2;
        assert_eq!(est, expect);
        // About a gibibyte per pass plus 160 MB of random writes.
        assert!(est > 2 * GIB && est < 3 * GIB);
    }

    #[test]
    fn profiles_scale_repetition_and_wear_but_not_the_block_sizes() {
        let q = DiskConfig::for_profile(BenchmarkProfile::Quick);
        let s = DiskConfig::for_profile(BenchmarkProfile::Standard);
        let e = DiskConfig::for_profile(BenchmarkProfile::Extended);
        assert!(q.file_bytes < s.file_bytes && s.file_bytes < e.file_bytes);
        assert!(
            q.seq_write_passes <= s.seq_write_passes && s.seq_write_passes <= e.seq_write_passes
        );
        // Never an unbounded number of write passes.
        assert!(e.seq_write_passes <= 3);
        assert_eq!(q.read_concurrency, vec![1, 8, 32]);
        assert_eq!(q.write_concurrency, vec![1, 32]);
        assert_eq!((SEQ_BLOCK, RAND_BLOCK), (1 << 20, 4096));
    }
}
