//! The CPU workloads of `cpu-method-v1`. Pure, deterministic, OS-independent.
//!
//! Every function does an exact amount of work for its arguments, polls the
//! cancel token at chunk boundaries (about 0.2 ms apart) and returns a
//! checksum of its result. Callers pass that checksum through
//! `std::hint::black_box` and verify it is identical on every iteration, so
//! the optimiser cannot delete the work and a miscompute is noticed.
//!
//! Changing anything here changes what a score means: bump the method id.

use std::io::{Read, Write};

use flate2::Compression;
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use sha2::{Digest, Sha256};

use super::super::cancel::CancelToken;

/// Independent dependency chains per workload, so the work is limited by
/// execution throughput rather than by one chain's latency.
pub const LANES: usize = 8;
/// Integer operations per step across all lanes: multiply, add, rotate, xor.
pub const INT_OPS_PER_STEP: u64 = 4 * LANES as u64;
/// Floating-point operations per step across all lanes: two multiplies and
/// one subtract.
pub const FLOAT_OPS_PER_STEP: u64 = 3 * LANES as u64;
/// Steps between cancellation checks.
const CHUNK_STEPS: u64 = 1 << 12;
/// Logistic-map parameter in the chaotic regime; keeps values inside (0, 1).
const LOGISTIC_R: f64 = 3.9;

#[derive(Debug, PartialEq, Eq)]
pub enum WorkloadError {
    Cancelled,
    /// The workload produced a result that cannot be right.
    Invalid(&'static str),
}

pub type WorkResult = Result<u64, WorkloadError>;

/// Fixed-seed generator for building inputs.
pub struct SplitMix64(pub u64);

impl SplitMix64 {
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

/// `steps` steps of eight independent multiply-add-rotate-xor chains.
pub fn integer(steps: u64, seed: u64, cancel: &CancelToken) -> WorkResult {
    let mut rng = SplitMix64(seed);
    let mut x = [0u64; LANES];
    for lane in &mut x {
        *lane = rng.next_u64() | 1;
    }
    let mut done = 0;
    while done < steps {
        if cancel.is_cancelled() {
            return Err(WorkloadError::Cancelled);
        }
        let n = CHUNK_STEPS.min(steps - done);
        for _ in 0..n {
            for lane in &mut x {
                let v = lane
                    .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                    .wrapping_add(0xD1B5_4A32_D192_ED03);
                *lane = v.rotate_left(29) ^ v;
            }
        }
        done += n;
    }
    Ok(x.iter().fold(0, |acc, v| acc.rotate_left(7) ^ v))
}

/// `steps` steps of eight independent logistic maps `x = r·x·(1−x)`.
pub fn floating_point(steps: u64, seed: u64, cancel: &CancelToken) -> WorkResult {
    let mut rng = SplitMix64(seed);
    let mut x = [0f64; LANES];
    for lane in &mut x {
        // 0.1 ..= 0.9, away from the fixed points at 0 and 1.
        *lane = 0.1 + 0.8 * ((rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64);
    }
    let mut done = 0;
    while done < steps {
        if cancel.is_cancelled() {
            return Err(WorkloadError::Cancelled);
        }
        let n = CHUNK_STEPS.min(steps - done);
        for _ in 0..n {
            for lane in &mut x {
                *lane = LOGISTIC_R * *lane * (1.0 - *lane);
            }
        }
        done += n;
    }
    // A NaN, an infinity or a collapse to 0 or 1 would mean the arithmetic
    // is not doing what the method defines.
    if x.iter().any(|v| !v.is_finite() || *v <= 0.0 || *v >= 1.0) {
        return Err(WorkloadError::Invalid("floating-point state left (0, 1)"));
    }
    Ok(x.iter().fold(0, |acc, v| acc.rotate_left(7) ^ v.to_bits()))
}

/// Deterministic pseudo-random bytes.
pub fn random_bytes(len: usize, seed: u64) -> Vec<u8> {
    let mut rng = SplitMix64(seed);
    let mut out = Vec::with_capacity(len + 8);
    while out.len() < len {
        out.extend_from_slice(&rng.next_u64().to_le_bytes());
    }
    out.truncate(len);
    out
}

/// `rounds` chained SHA-256 passes over `buf`: `d = SHA256(d ‖ buf)`, with
/// `d` starting as `seed` (little-endian) padded with zeros. Returns the first
/// eight bytes of the final digest.
pub fn hash(rounds: u64, buf: &[u8], seed: u64, cancel: &CancelToken) -> WorkResult {
    let mut digest = [0u8; 32];
    digest[..8].copy_from_slice(&seed.to_le_bytes());
    for _ in 0..rounds {
        if cancel.is_cancelled() {
            return Err(WorkloadError::Cancelled);
        }
        let mut h = Sha256::new();
        h.update(digest);
        h.update(buf);
        digest = h.finalize().into();
    }
    let mut first = [0u8; 8];
    first.copy_from_slice(&digest[..8]);
    Ok(u64::from_be_bytes(first))
}

const WORDS: [&str; 64] = [
    "the", "of", "and", "to", "in", "is", "that", "for", "it", "as", "was", "with", "be", "by",
    "on", "not", "he", "this", "are", "or", "his", "from", "at", "which", "but", "have", "an",
    "had", "they", "you", "were", "their", "one", "all", "we", "can", "her", "has", "there",
    "been", "if", "more", "when", "will", "would", "who", "so", "no", "system", "process",
    "memory", "thread", "kernel", "buffer", "cache", "device", "driver", "network", "packet",
    "socket", "file", "block", "queue", "clock",
];

/// Deterministic, semi-compressible text (roughly 3:1 with DEFLATE).
pub fn text_buffer(len: usize, seed: u64) -> Vec<u8> {
    let mut rng = SplitMix64(seed);
    let mut out = Vec::with_capacity(len + 16);
    while out.len() < len {
        let r = rng.next_u64();
        // Zipf-ish: favour the first words so there is real redundancy.
        let idx = ((r >> 8) as usize % WORDS.len()).min((r >> 40) as usize % WORDS.len());
        out.extend_from_slice(WORDS[idx].as_bytes());
        out.push(if r & 15 == 0 { b'\n' } else { b' ' });
    }
    out.truncate(len);
    out
}

pub fn deflate(data: &[u8]) -> Vec<u8> {
    let mut enc = DeflateEncoder::new(Vec::with_capacity(data.len() / 2), Compression::new(6));
    // Writing to a Vec cannot fail.
    let _ = enc.write_all(data);
    enc.finish().unwrap_or_default()
}

pub fn inflate(compressed: &[u8], expected_len: usize) -> Result<Vec<u8>, WorkloadError> {
    let mut out = Vec::with_capacity(expected_len);
    DeflateDecoder::new(compressed)
        .read_to_end(&mut out)
        .map_err(|_| WorkloadError::Invalid("inflate failed"))?;
    if out.len() != expected_len {
        return Err(WorkloadError::Invalid("inflate length mismatch"));
    }
    Ok(out)
}

/// Untimed check that compress then decompress reproduces `data` exactly.
pub fn verify_roundtrip(data: &[u8], compressed: &[u8]) -> Result<(), WorkloadError> {
    if inflate(compressed, data.len())? == data {
        Ok(())
    } else {
        Err(WorkloadError::Invalid("round trip differs"))
    }
}

/// Compresses `data` `reps` times; returns the compressed size.
pub fn compress_reps(data: &[u8], reps: u32, cancel: &CancelToken) -> WorkResult {
    let mut size = 0;
    for _ in 0..reps {
        if cancel.is_cancelled() {
            return Err(WorkloadError::Cancelled);
        }
        size = deflate(std::hint::black_box(data)).len() as u64;
    }
    Ok(size)
}

/// Decompresses `compressed` `reps` times; returns a cheap checksum of the
/// last output (its length and final bytes).
pub fn decompress_reps(
    compressed: &[u8],
    expected_len: usize,
    reps: u32,
    cancel: &CancelToken,
) -> WorkResult {
    let mut sum = 0;
    for _ in 0..reps {
        if cancel.is_cancelled() {
            return Err(WorkloadError::Cancelled);
        }
        let out = inflate(std::hint::black_box(compressed), expected_len)?;
        let tail = out
            .iter()
            .rev()
            .take(8)
            .fold(0u64, |a, b| (a << 8) | u64::from(*b));
        sum = (out.len() as u64) ^ tail;
    }
    Ok(sum)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token() -> CancelToken {
        CancelToken::new()
    }

    /// Straightforward reference for `integer`, with no chunking.
    fn integer_reference(steps: u64, seed: u64) -> u64 {
        let mut rng = SplitMix64(seed);
        let mut x: Vec<u64> = (0..LANES).map(|_| rng.next_u64() | 1).collect();
        for _ in 0..steps {
            for lane in x.iter_mut() {
                let v = lane
                    .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                    .wrapping_add(0xD1B5_4A32_D192_ED03);
                *lane = v.rotate_left(29) ^ v;
            }
        }
        x.iter().fold(0, |acc, v| acc.rotate_left(7) ^ v)
    }

    #[test]
    fn integer_is_deterministic_and_chunk_independent() {
        // Not a multiple of the chunk size, to exercise the remainder.
        let steps = 3 * CHUNK_STEPS + 17;
        let a = integer(steps, 7, &token()).unwrap();
        assert_eq!(a, integer(steps, 7, &token()).unwrap());
        assert_eq!(a, integer_reference(steps, 7));
        assert_ne!(a, integer(steps, 8, &token()).unwrap());
        assert_ne!(a, integer(steps + 1, 7, &token()).unwrap());
    }

    #[test]
    fn float_stays_valid_and_is_deterministic() {
        let a = floating_point(100_000, 11, &token()).unwrap();
        assert_eq!(a, floating_point(100_000, 11, &token()).unwrap());
        assert_ne!(a, floating_point(100_000, 12, &token()).unwrap());
        // Many more steps still produce a valid state on every seed tried.
        for seed in 0..32 {
            floating_point(50_000, seed, &token()).expect("valid state");
        }
    }

    #[test]
    fn hash_matches_the_sha256_reference_vector() {
        // One round over an empty buffer is SHA-256 of 32 zero bytes:
        // 66687aadf862bd77 6c8fc18b8e9f8e20 089714856ee233b3 902a591d0d5f2925
        assert_eq!(hash(1, &[], 0, &token()).unwrap(), 0x6668_7aad_f862_bd77);
        // Chaining depends on the previous digest and on the buffer.
        let buf = random_bytes(1024, 3);
        let two = hash(2, &buf, 0, &token()).unwrap();
        assert_ne!(two, hash(1, &buf, 0, &token()).unwrap());
        assert_eq!(two, hash(2, &buf, 0, &token()).unwrap());
        // Different chunk seeds give different chains.
        assert_ne!(two, hash(2, &buf, 1, &token()).unwrap());
    }

    #[test]
    fn buffers_are_deterministic() {
        assert_eq!(random_bytes(1000, 5), random_bytes(1000, 5));
        assert_ne!(random_bytes(1000, 5), random_bytes(1000, 6));
        assert_eq!(random_bytes(0, 1).len(), 0);
        assert_eq!(text_buffer(5000, 9), text_buffer(5000, 9));
        assert_eq!(text_buffer(5000, 9).len(), 5000);
    }

    #[test]
    fn compression_round_trips_and_actually_compresses() {
        let data = text_buffer(256 * 1024, 42);
        let packed = deflate(&data);
        verify_roundtrip(&data, &packed).expect("round trip");
        let ratio = data.len() as f64 / packed.len() as f64;
        assert!(ratio > 1.5 && ratio < 20.0, "ratio {ratio}");
        assert_eq!(
            compress_reps(&data, 2, &token()).unwrap(),
            packed.len() as u64
        );
        assert!(decompress_reps(&packed, data.len(), 2, &token()).is_ok());
        // A wrong expected length is detected, not accepted.
        assert_eq!(
            decompress_reps(&packed, data.len() + 1, 1, &token()),
            Err(WorkloadError::Invalid("inflate length mismatch"))
        );
        // Corrupt input is detected.
        let mut bad = packed.clone();
        bad.truncate(bad.len() / 2);
        assert!(inflate(&bad, data.len()).is_err());
    }

    #[test]
    fn a_cancelled_token_stops_every_workload_immediately() {
        let t = token();
        t.cancel();
        let c = Err(WorkloadError::Cancelled);
        assert_eq!(integer(u64::MAX, 1, &t), c);
        assert_eq!(floating_point(u64::MAX, 1, &t), c);
        assert_eq!(hash(u64::MAX, &[0; 16], 0, &t), c);
        assert_eq!(compress_reps(&[1, 2, 3], u32::MAX, &t), c);
        assert_eq!(decompress_reps(&[], 0, u32::MAX, &t), c);
    }
}
