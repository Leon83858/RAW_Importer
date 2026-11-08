//! OS-specific primitives, isolated behind small functions.
//!
//! Only this module contains `cfg(target_os)` code paths. Linux and macOS
//! behaviour are covered by native tests; Windows still requires validation
//! on that platform (see TESTING.md).

use std::io;
use std::path::Path;

/// Returns a stable identity token when this platform exposes one on stable Rust.
/// A missing identity means callers must not reuse cached file content hashes.
pub fn stable_file_identity(metadata: &std::fs::Metadata) -> Option<String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;

        let device = metadata.dev();
        let inode = metadata.ino();
        if inode == 0 {
            None
        } else {
            Some(format!("unix:{device:x}:{inode:x}"))
        }
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        None
    }
}

/// Moves `from` to `to` **without ever replacing an existing `to`**.
///
/// Returns an error of kind [`io::ErrorKind::AlreadyExists`] if `to` exists.
/// Both paths must be in the same directory (the copier guarantees this), so
/// the operation never crosses filesystems.
///
/// Strategy:
/// 1. native no-replace rename (Linux `renameat2(RENAME_NOREPLACE)`,
///    macOS `renamex_np(RENAME_EXCL)`, Windows `MoveFileExW` without
///    `MOVEFILE_REPLACE_EXISTING`);
/// 2. if the filesystem does not support it: `hard_link` + remove source;
/// 3. if hard links are unsupported too (e.g. exFAT, FAT32): existence check
///    followed by a plain rename. Step 3 has a small race window against
///    *other processes* creating the same name; within RAW Importer, names are
///    reserved before copying, so concurrent workers cannot collide.
pub fn rename_noreplace(from: &Path, to: &Path) -> io::Result<()> {
    match native_rename_noreplace(from, to) {
        Ok(()) => return Ok(()),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => return Err(e),
        Err(e) if !is_unsupported(&e) => return Err(e),
        Err(_) => {}
    }
    match std::fs::hard_link(from, to) {
        Ok(()) => {
            std::fs::remove_file(from)?;
            return Ok(());
        }
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => return Err(e),
        Err(e) if !is_unsupported(&e) => return Err(e),
        Err(_) => {}
    }
    if to.symlink_metadata().is_ok() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "destination already exists",
        ));
    }
    std::fs::rename(from, to)
}

fn is_unsupported(e: &io::Error) -> bool {
    if e.kind() == io::ErrorKind::Unsupported {
        return true;
    }
    #[cfg(unix)]
    {
        matches!(
            e.raw_os_error(),
            Some(libc::ENOSYS) | Some(libc::EINVAL) | Some(libc::ENOTSUP) | Some(libc::EPERM)
        )
    }
    #[cfg(not(unix))]
    {
        false
    }
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn native_rename_noreplace(from: &Path, to: &Path) -> io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let f = CString::new(from.as_os_str().as_bytes())?;
    let t = CString::new(to.as_os_str().as_bytes())?;
    // SAFETY: both pointers are valid NUL-terminated strings for the call.
    let rc = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            f.as_ptr(),
            libc::AT_FDCWD,
            t.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(target_os = "macos")]
fn native_rename_noreplace(from: &Path, to: &Path) -> io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let f = CString::new(from.as_os_str().as_bytes())?;
    let t = CString::new(to.as_os_str().as_bytes())?;
    // SAFETY: both pointers are valid NUL-terminated strings for the call.
    let rc = unsafe { libc::renamex_np(f.as_ptr(), t.as_ptr(), libc::RENAME_EXCL) };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(windows)]
fn native_rename_noreplace(from: &Path, to: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_WRITE_THROUGH};
    let wide = |p: &Path| -> Vec<u16> { p.as_os_str().encode_wide().chain(Some(0)).collect() };
    let (f, t) = (wide(from), wide(to));
    // Without MOVEFILE_REPLACE_EXISTING the call fails with ERROR_ALREADY_EXISTS /
    // ERROR_FILE_EXISTS if `to` exists; std maps both to ErrorKind::AlreadyExists.
    // SAFETY: both buffers are valid NUL-terminated UTF-16 strings for the call.
    let ok = unsafe { MoveFileExW(f.as_ptr(), t.as_ptr(), MOVEFILE_WRITE_THROUGH) };
    if ok != 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(not(any(
    all(target_os = "linux", target_env = "gnu"),
    target_os = "macos",
    windows
)))]
fn native_rename_noreplace(_from: &Path, _to: &Path) -> io::Result<()> {
    Err(io::Error::from(io::ErrorKind::Unsupported))
}

/// Flushes directory metadata (new entry) to disk where the OS supports it.
/// Failures are ignored: not every filesystem allows opening directories.
pub fn sync_dir(dir: &Path) {
    #[cfg(unix)]
    if let Ok(f) = std::fs::File::open(dir) {
        let _ = f.sync_all();
    }
    #[cfg(not(unix))]
    let _ = dir;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_replaces_existing_target() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        std::fs::write(&a, b"new").unwrap();
        std::fs::write(&b, b"old").unwrap();
        let err = rename_noreplace(&a, &b).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(&b).unwrap(), b"old");
        assert!(a.exists());
    }

    #[test]
    fn moves_when_target_absent() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        std::fs::write(&a, b"x").unwrap();
        rename_noreplace(&a, &b).unwrap();
        assert!(!a.exists());
        assert_eq!(std::fs::read(&b).unwrap(), b"x");
    }

    #[cfg(unix)]
    #[test]
    fn stable_file_identity_distinguishes_files() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("first");
        let second = dir.path().join("second");
        std::fs::write(&first, b"same bytes").unwrap();
        std::fs::write(&second, b"same bytes").unwrap();

        let first_metadata = std::fs::metadata(&first).unwrap();
        let second_metadata = std::fs::metadata(&second).unwrap();
        let first_id = stable_file_identity(&first_metadata).unwrap();
        assert_eq!(first_id, stable_file_identity(&first_metadata).unwrap());
        assert_ne!(first_id, stable_file_identity(&second_metadata).unwrap());
    }
}
