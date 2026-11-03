//! A heap buffer aligned for `O_DIRECT`.
//!
//! Direct I/O requires the buffer address, the file offset and the length to
//! be multiples of the device's logical block size. 4096 is a multiple of
//! both the 512-byte and 4 KiB sector sizes in use, so it is always safe. The
//! unsafe allocation is confined to this file and covered by tests.

use std::alloc::{Layout, alloc_zeroed, dealloc};

/// Alignment used for every direct-I/O buffer, offset and length.
pub const ALIGN: usize = 4096;

pub struct AlignedBuf {
    ptr: *mut u8,
    len: usize,
}

// SAFETY: the buffer owns its allocation exclusively; moving it to another
// thread moves the only pointer to it.
unsafe impl Send for AlignedBuf {}

impl AlignedBuf {
    /// A zeroed buffer of `len` bytes (rounded up to a multiple of
    /// [`ALIGN`]). `None` if the allocation fails.
    pub fn new(len: usize) -> Option<Self> {
        let len = len.max(1).checked_next_multiple_of(ALIGN)?;
        let layout = Layout::from_size_align(len, ALIGN).ok()?;
        // SAFETY: the layout has a non-zero size and a valid power-of-two
        // alignment.
        let ptr = unsafe { alloc_zeroed(layout) };
        (!ptr.is_null()).then_some(Self { ptr, len })
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn as_slice(&self) -> &[u8] {
        // SAFETY: `ptr` points to `len` initialised (zeroed) bytes that live
        // as long as `self`, and `&self` prevents concurrent mutation.
        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        // SAFETY: as above, and `&mut self` makes this the only reference.
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) }
    }
}

impl Drop for AlignedBuf {
    fn drop(&mut self) {
        // The same layout was validated in `new`.
        if let Ok(layout) = Layout::from_size_align(self.len, ALIGN) {
            // SAFETY: `ptr` was allocated with exactly this layout.
            unsafe { dealloc(self.ptr, layout) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_and_length_are_aligned_and_zeroed() {
        for len in [1, 511, 512, 4095, 4096, 4097, 1 << 20] {
            let b = AlignedBuf::new(len).expect("allocates");
            assert_eq!(b.as_slice().as_ptr() as usize % ALIGN, 0, "len {len}");
            assert_eq!(b.len() % ALIGN, 0);
            assert!(b.len() >= len);
            assert!(b.as_slice().iter().all(|x| *x == 0));
        }
    }

    #[test]
    fn contents_can_be_written_and_read_back() {
        let mut b = AlignedBuf::new(8192).unwrap();
        b.as_mut_slice()[0] = 7;
        b.as_mut_slice()[8191] = 9;
        assert_eq!((b.as_slice()[0], b.as_slice()[8191]), (7, 9));
    }

    #[test]
    fn can_move_between_threads() {
        let mut b = AlignedBuf::new(4096).unwrap();
        b.as_mut_slice()[10] = 42;
        let v = std::thread::spawn(move || b.as_slice()[10]).join().unwrap();
        assert_eq!(v, 42);
    }

    #[test]
    fn an_absurd_request_fails_instead_of_aborting() {
        assert!(AlignedBuf::new(usize::MAX / 2).is_none());
        assert!(AlignedBuf::new(usize::MAX).is_none());
        assert!(AlignedBuf::new(usize::MAX - 1).is_none());
    }
}
