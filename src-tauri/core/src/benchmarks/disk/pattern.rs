//! Data written by the disk benchmark and how it is checked.
//!
//! A block is a fixed pseudo-random base (incompressible, so drives or
//! filesystems that compress cannot flatter the result) with a 16-byte header
//! holding the block's file offset and a salt. Stamping the header is cheap
//! enough not to become the bottleneck on a fast NVMe drive, yet any block
//! read back can be recognised as the right data at the right place.

/// Header bytes at the start of every block: offset then salt, little-endian.
pub const HEADER: usize = 16;

/// Deterministic incompressible bytes.
pub fn fill_random(buf: &mut [u8], seed: u64) {
    let mut state = seed;
    for chunk in buf.chunks_mut(8) {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        let bytes = z.to_le_bytes();
        chunk.copy_from_slice(&bytes[..chunk.len()]);
    }
}

/// Writes the offset and salt into the first bytes of `block`.
pub fn stamp(block: &mut [u8], offset: u64, salt: u64) {
    block[..8].copy_from_slice(&offset.to_le_bytes());
    block[8..HEADER].copy_from_slice(&salt.to_le_bytes());
}

/// Whether the header of `block` names this offset and salt. This is the
/// cheap check done on every block inside the timed loops.
pub fn header_matches(block: &[u8], offset: u64, salt: u64) -> bool {
    block.len() >= HEADER
        && block[..8] == offset.to_le_bytes()
        && block[8..HEADER] == salt.to_le_bytes()
}

/// Whether `block` is exactly `base` with the header for this offset and
/// salt: the full check, done outside the timed loops on sampled blocks.
pub fn block_matches(block: &[u8], base: &[u8], offset: u64, salt: u64) -> bool {
    header_matches(block, offset, salt)
        && base.len() >= block.len()
        && block[HEADER..] == base[HEADER..block.len()]
}

/// Granularity of the data pattern: every 4 KiB unit carries its own header,
/// so a random 4 KiB read anywhere can be recognised.
pub const UNIT: usize = 4096;

/// Fills every unit of `buf` with the 4 KiB `base` block.
pub fn init_units(buf: &mut [u8], base: &[u8]) {
    for chunk in buf.chunks_mut(UNIT) {
        chunk.copy_from_slice(&base[..chunk.len()]);
    }
}

/// Stamps every unit of `buf`, which will be written at file offset
/// `base_offset`.
pub fn stamp_units(buf: &mut [u8], base_offset: u64, salt: u64) {
    for (i, chunk) in buf.chunks_mut(UNIT).enumerate() {
        stamp(chunk, base_offset + (i * UNIT) as u64, salt);
    }
}

/// Offset of the first unit whose header is wrong (the cheap check run on
/// everything read in the timed loops), or `None` when all are right.
pub fn first_bad_header(buf: &[u8], base_offset: u64, salt: u64) -> Option<u64> {
    buf.chunks(UNIT).enumerate().find_map(|(i, c)| {
        let off = base_offset + (i * UNIT) as u64;
        (!header_matches(c, off, salt)).then_some(off)
    })
}

/// Offset of the first unit that is not exactly `base` plus its header (the
/// full check, run on sampled blocks outside the timed loops).
pub fn first_bad_unit(buf: &[u8], base: &[u8], base_offset: u64, salt: u64) -> Option<u64> {
    buf.chunks(UNIT).enumerate().find_map(|(i, c)| {
        let off = base_offset + (i * UNIT) as u64;
        (!block_matches(c, base, off, salt)).then_some(off)
    })
}

/// Salt for the data written in write pass `pass`.
pub fn pass_salt(pass: u32) -> u64 {
    0x7461_736B_6D67_7200 ^ u64::from(pass)
}

/// Salt for random-write data.
pub const RANDOM_WRITE_SALT: u64 = 0x7261_6E64_7772_6974;

/// Fast deterministic generator for choosing offsets.
pub struct Rng(pub u64);

impl Rng {
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform-enough value in `0..n` (bias is negligible for a benchmark).
    pub fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n.max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_data_is_deterministic_and_incompressible_looking() {
        let (mut a, mut b) = (vec![0u8; 1001], vec![0u8; 1001]);
        fill_random(&mut a, 5);
        fill_random(&mut b, 5);
        assert_eq!(a, b);
        fill_random(&mut b, 6);
        assert_ne!(a, b);
        // Odd length is filled completely; bytes are not mostly zero.
        assert!(a.iter().filter(|x| **x == 0).count() < 30);
    }

    #[test]
    fn a_stamped_block_is_recognised_only_at_its_own_offset_and_salt() {
        let mut base = vec![0u8; 4096];
        fill_random(&mut base, 1);
        let mut block = base.clone();
        stamp(&mut block, 8192, 77);
        assert!(header_matches(&block, 8192, 77));
        assert!(block_matches(&block, &base, 8192, 77));
        assert!(!header_matches(&block, 4096, 77), "wrong offset");
        assert!(!header_matches(&block, 8192, 78), "wrong salt");
    }

    #[test]
    fn corruption_anywhere_in_the_block_is_caught_by_the_full_check() {
        let mut base = vec![0u8; 4096];
        fill_random(&mut base, 2);
        let mut block = base.clone();
        stamp(&mut block, 0, 1);
        assert!(block_matches(&block, &base, 0, 1));
        for at in [0, 15, 16, 2000, 4095] {
            let mut bad = block.clone();
            bad[at] ^= 1;
            assert!(!block_matches(&bad, &base, 0, 1), "flip at {at}");
        }
        // All zeros (an unwritten or sparse region) never passes.
        assert!(!block_matches(&vec![0u8; 4096], &base, 0, 1));
        // The cheap check misses a flip beyond the header, by design.
        let mut tail = block.clone();
        tail[3000] ^= 1;
        assert!(header_matches(&tail, 0, 1));
    }

    #[test]
    fn every_unit_of_a_large_buffer_is_identifiable() {
        let mut base = vec![0u8; UNIT];
        fill_random(&mut base, 3);
        let mut buf = vec![0u8; 4 * UNIT];
        init_units(&mut buf, &base);
        stamp_units(&mut buf, 1 << 20, 9);
        assert_eq!(first_bad_header(&buf, 1 << 20, 9), None);
        assert_eq!(first_bad_unit(&buf, &base, 1 << 20, 9), None);
        // The same data claimed for another place or pass is wrong from the start.
        assert_eq!(first_bad_header(&buf, 0, 9), Some(0));
        assert_eq!(first_bad_header(&buf, 1 << 20, 8), Some(1 << 20));
        // Damage in the third unit is reported at that unit's offset.
        buf[2 * UNIT + 100] ^= 0xFF;
        assert_eq!(
            first_bad_header(&buf, 1 << 20, 9),
            None,
            "cheap check misses body damage"
        );
        assert_eq!(
            first_bad_unit(&buf, &base, 1 << 20, 9),
            Some((1 << 20) + 2 * UNIT as u64)
        );
        // A header lost in the last unit is caught by both.
        buf[2 * UNIT + 100] ^= 0xFF;
        buf[3 * UNIT] ^= 1;
        assert_eq!(
            first_bad_header(&buf, 1 << 20, 9),
            Some((1 << 20) + 3 * UNIT as u64)
        );
    }

    #[test]
    fn passes_use_different_salts() {
        assert_ne!(pass_salt(0), pass_salt(1));
        assert_ne!(pass_salt(0), RANDOM_WRITE_SALT);
    }

    #[test]
    fn rng_is_deterministic_and_bounded() {
        let (mut a, mut b) = (Rng(9), Rng(9));
        assert_eq!(a.next_u64(), b.next_u64());
        let mut r = Rng(1);
        for _ in 0..1000 {
            assert!(r.below(10) < 10);
        }
        assert_eq!(r.below(0), 0);
    }
}
