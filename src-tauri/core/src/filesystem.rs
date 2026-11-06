//! Private, bounded local persistence. Parent directories must belong to the
//! current user; temporary files are created exclusively beside the destination.

use std::fs::{DirBuilder, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;

pub fn private_dir(path: &Path) -> io::Result<()> {
    let mut builder = DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)
}

/// Replaces the directory entry, never truncating a symlink's target. A failed
/// write leaves the previous file intact and the temporary file is removed.
pub fn write_private_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("missing parent"))?;
    private_dir(parent)?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    tmp.write_all(contents)?;
    tmp.as_file().sync_all()?;
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

/// Rejects special files and symlinks and enforces the bound while reading,
/// including when the file grows after it is opened.
pub fn read_text(path: &Path, max_bytes: u64) -> io::Result<String> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x0020_0000); // FILE_FLAG_OPEN_REPARSE_POINT
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(io::Error::other("reparse points are not accepted"));
        }
    }
    if !metadata.is_file() || metadata.len() > max_bytes {
        return Err(io::Error::other("not a regular file within the size limit"));
    }
    let mut text = String::new();
    file.take(max_bytes.saturating_add(1))
        .read_to_string(&mut text)?;
    if text.len() as u64 > max_bytes {
        return Err(io::Error::other("file exceeds the size limit"));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_reads_and_replaces_files_without_leaving_temporary_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.json");
        write_private_atomic(&path, b"old").unwrap();
        assert_eq!(read_text(&path, 3).unwrap(), "old");
        assert!(read_text(&path, 2).is_err());
        write_private_atomic(&path, b"new").unwrap();
        assert_eq!(read_text(&path, 3).unwrap(), "new");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlink_reads_and_never_truncates_the_target_on_save() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("keep");
        let link = dir.path().join("settings.json");
        std::fs::write(&target, "keep me").unwrap();
        symlink(&target, &link).unwrap();
        assert!(read_text(&link, 100).is_err());
        write_private_atomic(&link, b"settings").unwrap();
        assert_eq!(std::fs::read_to_string(target).unwrap(), "keep me");
        assert_eq!(read_text(&link, 100).unwrap(), "settings");
        assert_eq!(
            std::fs::metadata(link).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[cfg(unix)]
    #[test]
    fn refuses_fifos_without_waiting_for_a_writer() {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fifo");
        let name = CString::new(path.as_os_str().as_bytes()).unwrap();
        // SAFETY: name is a valid NUL-terminated path in our test directory.
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        assert!(read_text(&path, 100).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn denied_file_and_directory_access_preserve_existing_data() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keep");
        std::fs::write(&path, "keep me").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
        let read = read_text(&path, 100);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o500)).unwrap();
        let save = write_private_atomic(&path, b"replacement");
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(read.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(save.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(std::fs::read_to_string(path).unwrap(), "keep me");
    }
}
