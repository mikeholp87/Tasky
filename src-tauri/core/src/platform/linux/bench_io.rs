//! Storage-benchmark file I/O with `O_DIRECT`, which bypasses the page
//! cache. Some filesystems (tmpfs, some FUSE/overlay mounts) reject it; the
//! caller then falls back to buffered I/O and labels the result.

use std::fs::{File, OpenOptions};
use std::io;
use std::os::unix::fs::{FileExt, OpenOptionsExt};
use std::path::Path;

pub fn open_benchmark_file(path: &Path, create: bool, direct: bool) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    if create {
        options.create_new(true);
    }
    if direct {
        options.custom_flags(libc::O_DIRECT);
    }
    options.open(path)
}

pub fn read_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<usize> {
    file.read_at(buf, offset)
}

pub fn write_at(file: &File, buf: &[u8], offset: u64) -> io::Result<usize> {
    file.write_at(buf, offset)
}
