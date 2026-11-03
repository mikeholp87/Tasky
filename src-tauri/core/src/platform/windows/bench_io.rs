//! Storage-benchmark file I/O: unbuffered, write-through access so results
//! reflect the drive rather than the system file cache.
//!
//! `FILE_FLAG_NO_BUFFERING` requires offsets, sizes and buffer addresses
//! aligned to the volume's sector size; callers use 4 KiB alignment, which
//! covers both 512-byte and 4 KiB sector drives.

use std::fs::{File, OpenOptions};
use std::io;
use std::os::windows::fs::{FileExt, OpenOptionsExt};
use std::path::Path;

const FILE_FLAG_NO_BUFFERING: u32 = 0x2000_0000;
const FILE_FLAG_WRITE_THROUGH: u32 = 0x8000_0000;

pub fn open_benchmark_file(path: &Path, create: bool, direct: bool) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    if create {
        // Never truncate or reuse an existing file.
        options.create_new(true);
    }
    if direct {
        options.custom_flags(FILE_FLAG_NO_BUFFERING | FILE_FLAG_WRITE_THROUGH);
    }
    options.open(path)
}

pub fn read_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<usize> {
    file.seek_read(buf, offset)
}

pub fn write_at(file: &File, buf: &[u8], offset: u64) -> io::Result<usize> {
    file.seek_write(buf, offset)
}
