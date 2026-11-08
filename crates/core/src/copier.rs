//! Safe single-file copy (master prompt §12, ARCHITECTURE.md §5).
//!
//! 1. snapshot source identity (size, mtime)
//! 2. create a temp file next to the final destination (`create_new`)
//! 3. stream-copy, checking cancellation per chunk, optionally hashing
//! 4. fsync, verify byte count, optionally verify hash of the written file
//! 5. re-check source identity (detect changes during copy)
//! 6. preserve modification time, then finalise **without replacing** anything
//!
//! The source file is opened read-only and never modified or deleted.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use sha2::{Digest, Sha256};

use crate::error::{CoreError, Result};
use crate::platform;
use crate::state::Control;

pub const DEFAULT_BUFFER: usize = 1024 * 1024;
/// Marker contained in every temp file name, used by recovery to recognise
/// our own leftovers.
pub const TEMP_MARKER: &str = ".rawimp-";

#[derive(Debug, Clone)]
pub struct CopyOptions {
    /// Re-read the written file and compare its SHA-256 with the source stream.
    pub verify_hash: bool,
    /// Set the destination's modification time to the source's (fixes 2.x B6).
    pub preserve_mtime: bool,
    pub buffer_size: usize,
    /// Identifies the session in temp file names (attributable leftovers).
    pub session_tag: String,
}

impl Default for CopyOptions {
    fn default() -> Self {
        CopyOptions {
            verify_hash: false,
            preserve_mtime: true,
            buffer_size: DEFAULT_BUFFER,
            session_tag: "manual".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyReport {
    pub bytes: u64,
    pub sha256: Option<[u8; 32]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Identity {
    len: u64,
    modified: Option<SystemTime>,
}

fn identity(path: &Path) -> Result<Identity> {
    let m = std::fs::metadata(path).map_err(|e| CoreError::io(path, e))?;
    Ok(Identity {
        len: m.len(),
        modified: m.modified().ok(),
    })
}

pub fn temp_path_for(final_path: &Path, session_tag: &str) -> PathBuf {
    let name = final_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    final_path.with_file_name(format!(".{name}{TEMP_MARKER}{session_tag}-{nonce:x}.part"))
}

/// Removes our own temp file; never touches anything else.
fn discard(temp: &Path) {
    if let Err(e) = std::fs::remove_file(temp) {
        tracing::warn!(path = %temp.display(), error = %e, "could not remove temp file");
    }
}

/// Copies `src` to `final_path`. The parent directory must exist.
///
/// `on_bytes` receives the number of bytes written since the previous call.
/// Returns [`CoreError::DestinationExists`] if `final_path` appeared in the
/// meantime; the caller then re-plans the name.
pub fn copy_file(
    src: &Path,
    final_path: &Path,
    opts: &CopyOptions,
    control: &Control,
    mut on_bytes: impl FnMut(u64),
) -> Result<CopyReport> {
    let before = identity(src)?;
    let mut input = File::open(src).map_err(|e| CoreError::io(src, e))?;

    let temp = temp_path_for(final_path, &opts.session_tag);
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|e| CoreError::io(&temp, e))?;

    let result = (|| -> Result<CopyReport> {
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; opts.buffer_size.max(4096)];
        let mut written: u64 = 0;
        loop {
            if control.is_cancelled() {
                return Err(CoreError::Cancelled);
            }
            let n = match input.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(CoreError::io(src, e)),
            };
            output
                .write_all(&buf[..n])
                .map_err(|e| CoreError::io(&temp, e))?;
            if opts.verify_hash {
                hasher.update(&buf[..n]);
            }
            written += n as u64;
            on_bytes(n as u64);
        }
        output.sync_all().map_err(|e| CoreError::io(&temp, e))?;
        drop(output);

        if written != before.len {
            // Either the source changed size or a short read occurred.
            let after = identity(src)?;
            if after != before {
                return Err(CoreError::SourceChanged(src.to_path_buf()));
            }
            return Err(CoreError::SizeMismatch {
                path: src.to_path_buf(),
                expected: before.len,
                written,
            });
        }
        let on_disk = std::fs::metadata(&temp)
            .map_err(|e| CoreError::io(&temp, e))?
            .len();
        if on_disk != written {
            return Err(CoreError::SizeMismatch {
                path: temp.clone(),
                expected: written,
                written: on_disk,
            });
        }
        if identity(src)? != before {
            return Err(CoreError::SourceChanged(src.to_path_buf()));
        }

        let sha256 = if opts.verify_hash {
            let expected: [u8; 32] = hasher.finalize().into();
            let actual = hash_file(&temp, opts.buffer_size, control)?;
            if actual != expected {
                return Err(CoreError::HashMismatch(final_path.to_path_buf()));
            }
            Some(expected)
        } else {
            None
        };

        if opts.preserve_mtime {
            if let Some(m) = before.modified {
                let ft = filetime::FileTime::from_system_time(m);
                filetime::set_file_mtime(&temp, ft).map_err(|e| CoreError::io(&temp, e))?;
            }
        }

        platform::rename_noreplace(&temp, final_path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                CoreError::DestinationExists(final_path.to_path_buf())
            } else {
                CoreError::io(final_path, e)
            }
        })?;
        if let Some(dir) = final_path.parent() {
            platform::sync_dir(dir);
        }
        Ok(CopyReport {
            bytes: written,
            sha256,
        })
    })();

    if result.is_err() && temp.exists() {
        discard(&temp);
    }
    result
}

/// Streams a file through SHA-256 with bounded memory.
pub fn hash_file(path: &Path, buffer_size: usize, control: &Control) -> Result<[u8; 32]> {
    let mut f = File::open(path).map_err(|e| CoreError::io(path, e))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; buffer_size.max(4096)];
    loop {
        if control.is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        match f.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => hasher.update(&buf[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(CoreError::io(path, e)),
        }
    }
    Ok(hasher.finalize().into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> CopyOptions {
        CopyOptions {
            verify_hash: true,
            buffer_size: 4096,
            session_tag: "test".into(),
            ..Default::default()
        }
    }

    fn leftovers(dir: &Path) -> Vec<String> {
        std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(TEMP_MARKER))
            .collect()
    }

    #[test]
    fn copies_bytes_hash_and_mtime() {
        let t = tempfile::tempdir().unwrap();
        let src = t.path().join("a.arw");
        let data: Vec<u8> = (0..100_000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(&src, &data).unwrap();
        let old = filetime::FileTime::from_unix_time(1_600_000_000, 0);
        filetime::set_file_mtime(&src, old).unwrap();

        let dst = t.path().join("out.arw");
        let mut progressed = 0;
        let r = copy_file(&src, &dst, &opts(), &Control::default(), |n| {
            progressed += n
        })
        .unwrap();
        assert_eq!(r.bytes, data.len() as u64);
        assert_eq!(progressed, data.len() as u64);
        assert_eq!(std::fs::read(&dst).unwrap(), data);
        assert!(r.sha256.is_some());
        let m = filetime::FileTime::from_last_modification_time(&std::fs::metadata(&dst).unwrap());
        assert_eq!(m.unix_seconds(), old.unix_seconds());
        assert!(leftovers(t.path()).is_empty());
        // Source untouched.
        assert_eq!(std::fs::read(&src).unwrap(), data);
    }

    #[test]
    fn refuses_to_replace_existing_destination() {
        let t = tempfile::tempdir().unwrap();
        let src = t.path().join("a");
        let dst = t.path().join("b");
        std::fs::write(&src, b"new").unwrap();
        std::fs::write(&dst, b"old").unwrap();
        let err = copy_file(&src, &dst, &opts(), &Control::default(), |_| {}).unwrap_err();
        assert!(matches!(err, CoreError::DestinationExists(_)));
        assert_eq!(std::fs::read(&dst).unwrap(), b"old");
        assert!(leftovers(t.path()).is_empty());
    }

    #[test]
    fn cancellation_leaves_no_final_or_temp_file() {
        let t = tempfile::tempdir().unwrap();
        let src = t.path().join("a");
        std::fs::write(&src, vec![7u8; 200_000]).unwrap();
        let dst = t.path().join("b");
        let ctl = Control::default();
        let mut calls = 0;
        let err = copy_file(&src, &dst, &opts(), &ctl, |_| {
            calls += 1;
            if calls == 2 {
                ctl.cancel();
            }
        })
        .unwrap_err();
        assert!(matches!(err, CoreError::Cancelled));
        assert!(!dst.exists());
        assert!(leftovers(t.path()).is_empty());
    }

    #[test]
    fn detects_source_modified_during_copy() {
        let t = tempfile::tempdir().unwrap();
        let src = t.path().join("a");
        std::fs::write(&src, vec![1u8; 100_000]).unwrap();
        let dst = t.path().join("b");
        let src2 = src.clone();
        let mut done = false;
        let err = copy_file(&src, &dst, &opts(), &Control::default(), move |_| {
            if !done {
                done = true;
                let mut f = OpenOptions::new().append(true).open(&src2).unwrap();
                f.write_all(b"appended").unwrap();
            }
        })
        .unwrap_err();
        assert!(matches!(err, CoreError::SourceChanged(_)), "{err:?}");
        assert!(!dst.exists());
        assert!(leftovers(t.path()).is_empty());
    }

    #[test]
    fn missing_source_is_reported() {
        let t = tempfile::tempdir().unwrap();
        let err = copy_file(
            &t.path().join("x"),
            &t.path().join("y"),
            &opts(),
            &Control::default(),
            |_| {},
        )
        .unwrap_err();
        assert_eq!(err.code(), crate::error::ErrorCode::SourceVanished);
    }

    #[test]
    fn empty_file() {
        let t = tempfile::tempdir().unwrap();
        let src = t.path().join("e");
        std::fs::write(&src, b"").unwrap();
        let dst = t.path().join("f");
        let r = copy_file(&src, &dst, &opts(), &Control::default(), |_| {}).unwrap();
        assert_eq!(r.bytes, 0);
        assert!(dst.exists());
    }
}
