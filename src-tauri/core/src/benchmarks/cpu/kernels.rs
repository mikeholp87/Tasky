//! Deterministic CPU workloads.
//!
//! Each kernel processes one fixed-size *chunk* per call and returns the
//! amount of work done (operations, floating-point operations or bytes).
//! Chunks are small (tens to hundreds of microseconds) so cancellation is
//! checked often, and large enough that loop overhead is negligible.
//! Results feed back into kernel state and pass through
//! [`std::hint::black_box`], so the compiler cannot remove or precompute
//! the work.

use std::hint::black_box;
use std::sync::Arc;

use sha2::{Digest, Sha256};

/// 64 KiB of `u64`s: fits in L1/L2, so the integer test measures the core,
/// not the memory system.
const INT_TABLE_LEN: usize = 8 * 1024;
/// Integer operations per table element per chunk (xor, rotate, multiply,
/// shift, xor, add).
pub const INT_OPS_PER_ELEMENT: u64 = 6;

/// Independent (x, y) pairs rotated per step, enough for instruction-level
/// parallelism on wide cores.
const FLOAT_LANES: usize = 256;
const FLOAT_STEPS_PER_CHUNK: usize = 64;
/// FLOPs per lane per step: 4 multiplies + 2 additions/subtractions.
pub const FLOAT_OPS_PER_STEP: u64 = 6;

/// Bytes hashed / (de)compressed per chunk.
pub const BLOCK_BYTES: usize = 256 * 1024;
/// Input of the SHA-256 test (cycled through in blocks).
pub const SHA_INPUT_BYTES: usize = 4 * 1024 * 1024;
/// Input of the LZ4 tests.
pub const LZ4_INPUT_BYTES: usize = 8 * 1024 * 1024;

pub trait Kernel: Send {
    /// Runs one chunk and returns the work done in the kernel's work unit.
    fn chunk(&mut self) -> u64;
}

/// xorshift64* — small, fast, deterministic; not for cryptography.
#[derive(Debug, Clone)]
pub struct Prng(u64);

impl Prng {
    pub fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

/// Deterministic, moderately compressible data: a mix of tokens from a
/// small dictionary (repetition LZ4 can exploit) and random bytes.
pub fn benchmark_data(len: usize, seed: u64) -> Vec<u8> {
    let mut rng = Prng::new(seed);
    let dictionary: Vec<[u8; 8]> = (0..256).map(|_| rng.next_u64().to_le_bytes()).collect();
    let mut out = Vec::with_capacity(len + 8);
    while out.len() < len {
        let r = rng.next_u64();
        if r & 3 != 0 {
            out.extend_from_slice(&dictionary[(r >> 8) as usize & 0xFF]);
        } else {
            out.extend_from_slice(&r.to_le_bytes());
        }
    }
    out.truncate(len);
    out
}

pub struct IntegerKernel {
    table: Vec<u64>,
    acc: u64,
}

impl IntegerKernel {
    pub fn new(seed: u64) -> Self {
        let mut rng = Prng::new(seed);
        Self {
            table: (0..INT_TABLE_LEN).map(|_| rng.next_u64()).collect(),
            acc: seed,
        }
    }
}

impl Kernel for IntegerKernel {
    fn chunk(&mut self) -> u64 {
        let mut acc = black_box(self.acc);
        for x in &mut self.table {
            let mut v = *x ^ acc;
            v = v.rotate_left(13).wrapping_mul(0x9E37_79B9_7F4A_7C15);
            v ^= v >> 29;
            *x = v;
            acc = acc.wrapping_add(v);
        }
        self.acc = black_box(acc);
        INT_TABLE_LEN as u64 * INT_OPS_PER_ELEMENT
    }
}

/// Rotates 2-D points by a fixed angle. Rotation preserves length, so values
/// stay within [-1, 1] (re-normalized each chunk against rounding drift) and
/// can never become NaN or infinite.
pub struct FloatKernel {
    xs: Vec<f64>,
    ys: Vec<f64>,
    cos: f64,
    sin: f64,
}

impl FloatKernel {
    pub fn new(seed: u64) -> Self {
        let mut rng = Prng::new(seed);
        let mut xs = Vec::with_capacity(FLOAT_LANES);
        let mut ys = Vec::with_capacity(FLOAT_LANES);
        for _ in 0..FLOAT_LANES {
            let angle = (rng.next_u64() % 6_283) as f64 / 1_000.0;
            xs.push(angle.cos());
            ys.push(angle.sin());
        }
        let theta = black_box(0.001_f64);
        Self {
            xs,
            ys,
            cos: theta.cos(),
            sin: theta.sin(),
        }
    }

    pub fn max_magnitude(&self) -> f64 {
        self.xs
            .iter()
            .zip(&self.ys)
            .map(|(x, y)| x.hypot(*y))
            .fold(0.0, f64::max)
    }
}

impl Kernel for FloatKernel {
    fn chunk(&mut self) -> u64 {
        let (c, s) = (black_box(self.cos), black_box(self.sin));
        for _ in 0..FLOAT_STEPS_PER_CHUNK {
            for (x, y) in self.xs.iter_mut().zip(self.ys.iter_mut()) {
                let nx = *x * c - *y * s;
                let ny = *x * s + *y * c;
                *x = nx;
                *y = ny;
            }
        }
        // Not counted: keeps |(x, y)| = 1 despite rounding.
        for (x, y) in self.xs.iter_mut().zip(self.ys.iter_mut()) {
            let r = x.hypot(*y);
            if r > 0.0 {
                *x /= r;
                *y /= r;
            }
        }
        black_box(&mut self.xs);
        black_box(&mut self.ys);
        (FLOAT_LANES * FLOAT_STEPS_PER_CHUNK) as u64 * FLOAT_OPS_PER_STEP
    }
}

pub struct ShaKernel {
    data: Arc<[u8]>,
    pos: usize,
    last: [u8; 32],
}

impl ShaKernel {
    pub fn new(data: Arc<[u8]>) -> Self {
        Self {
            data,
            pos: 0,
            last: [0; 32],
        }
    }
}

impl Kernel for ShaKernel {
    fn chunk(&mut self) -> u64 {
        let end = (self.pos + BLOCK_BYTES).min(self.data.len());
        let block = &self.data[self.pos..end];
        let mut hasher = Sha256::new();
        hasher.update(black_box(block));
        // Chain the previous digest so every block depends on the last.
        hasher.update(self.last);
        self.last = black_box(hasher.finalize().into());
        self.pos = if end >= self.data.len() { 0 } else { end };
        block.len() as u64
    }
}

pub struct Lz4CompressKernel {
    data: Arc<[u8]>,
    out: Vec<u8>,
    pos: usize,
}

impl Lz4CompressKernel {
    pub fn new(data: Arc<[u8]>) -> Self {
        Self {
            data,
            out: vec![0; lz4_flex::block::get_maximum_output_size(BLOCK_BYTES)],
            pos: 0,
        }
    }
}

impl Kernel for Lz4CompressKernel {
    fn chunk(&mut self) -> u64 {
        let end = (self.pos + BLOCK_BYTES).min(self.data.len());
        let block = &self.data[self.pos..end];
        let n = lz4_flex::block::compress_into(black_box(block), &mut self.out).unwrap_or(0);
        black_box(&self.out[..n]);
        self.pos = if end >= self.data.len() { 0 } else { end };
        block.len() as u64
    }
}

/// Pre-compressed blocks of the benchmark data, shared by all workers.
pub struct CompressedBlocks {
    pub blocks: Vec<(Vec<u8>, usize)>,
    pub input_bytes: usize,
    pub compressed_bytes: usize,
}

impl CompressedBlocks {
    pub fn new(data: &[u8]) -> Self {
        let blocks: Vec<(Vec<u8>, usize)> = data
            .chunks(BLOCK_BYTES)
            .map(|b| (lz4_flex::block::compress(b), b.len()))
            .collect();
        Self {
            compressed_bytes: blocks.iter().map(|(c, _)| c.len()).sum(),
            input_bytes: data.len(),
            blocks,
        }
    }

    /// Original size ÷ compressed size.
    pub fn ratio(&self) -> f64 {
        self.input_bytes as f64 / self.compressed_bytes.max(1) as f64
    }
}

pub struct Lz4DecompressKernel {
    blocks: Arc<CompressedBlocks>,
    out: Vec<u8>,
    index: usize,
}

impl Lz4DecompressKernel {
    pub fn new(blocks: Arc<CompressedBlocks>) -> Self {
        Self {
            blocks,
            out: vec![0; BLOCK_BYTES],
            index: 0,
        }
    }
}

impl Kernel for Lz4DecompressKernel {
    fn chunk(&mut self) -> u64 {
        let Some((compressed, len)) = self.blocks.blocks.get(self.index) else {
            self.index = 0;
            return 0;
        };
        let n = lz4_flex::block::decompress_into(black_box(compressed), &mut self.out[..*len])
            .unwrap_or(0);
        black_box(&self.out[..n]);
        self.index = (self.index + 1) % self.blocks.blocks.len().max(1);
        n as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prng_and_data_are_deterministic() {
        assert_eq!(benchmark_data(4096, 7), benchmark_data(4096, 7));
        assert_ne!(benchmark_data(4096, 7), benchmark_data(4096, 8));
        assert_eq!(benchmark_data(1000, 1).len(), 1000);
    }

    #[test]
    fn integer_kernel_reports_exact_work_and_changes_state() {
        let mut k = IntegerKernel::new(1);
        let before = k.acc;
        assert_eq!(k.chunk(), INT_TABLE_LEN as u64 * INT_OPS_PER_ELEMENT);
        assert_ne!(k.acc, before);
        // Same seed, same result.
        let mut a = IntegerKernel::new(5);
        let mut b = IntegerKernel::new(5);
        a.chunk();
        b.chunk();
        assert_eq!(a.acc, b.acc);
    }

    #[test]
    fn float_kernel_stays_finite_and_bounded() {
        let mut k = FloatKernel::new(3);
        for _ in 0..2_000 {
            assert_eq!(
                k.chunk(),
                (FLOAT_LANES * FLOAT_STEPS_PER_CHUNK) as u64 * FLOAT_OPS_PER_STEP
            );
        }
        assert!(k.xs.iter().chain(&k.ys).all(|v| v.is_finite()));
        assert!((k.max_magnitude() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn sha_kernel_cycles_through_input() {
        let data: Arc<[u8]> = benchmark_data(BLOCK_BYTES * 2 + 100, 1).into();
        let mut k = ShaKernel::new(data);
        assert_eq!(k.chunk(), BLOCK_BYTES as u64);
        assert_eq!(k.chunk(), BLOCK_BYTES as u64);
        assert_eq!(k.chunk(), 100);
        assert_eq!(k.pos, 0);
        assert_ne!(k.last, [0; 32]);
    }

    #[test]
    fn lz4_round_trips_and_compresses() {
        let data = benchmark_data(BLOCK_BYTES * 3, 9);
        let blocks = Arc::new(CompressedBlocks::new(&data));
        let ratio = blocks.ratio();
        assert!(ratio > 1.2 && ratio < 10.0, "ratio {ratio}");
        let mut k = Lz4DecompressKernel::new(Arc::clone(&blocks));
        assert_eq!(k.chunk(), BLOCK_BYTES as u64);
        assert_eq!(&k.out[..], &data[..BLOCK_BYTES]);
        let mut c = Lz4CompressKernel::new(data.into());
        assert_eq!(c.chunk(), BLOCK_BYTES as u64);
    }
}
