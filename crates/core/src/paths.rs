//! Backend validation of source and destination folders (master prompt §7).
//!
//! The frontend may pre-validate for convenience, but only this module decides.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::ErrorCode;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct PathIssue {
    pub code: ErrorCode,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct ValidatedPaths {
    /// Canonical source directory (symlinks resolved once, here).
    pub source: PathBuf,
    /// Canonical destination directory, or the not-yet-existing destination
    /// below a canonical, existing parent.
    pub destination: PathBuf,
    /// `true` if the destination folder does not exist yet and will be created.
    pub destination_will_be_created: bool,
}

/// Validates both paths and returns *all* problems at once so the UI can show
/// them together.
pub fn validate(source: &Path, destination: &Path) -> Result<ValidatedPaths, Vec<PathIssue>> {
    let mut issues = Vec::new();
    let issue = |code, path: &Path| PathIssue {
        code,
        path: path.to_path_buf(),
    };

    let source_canon = match std::fs::metadata(source) {
        Err(_) => {
            issues.push(issue(ErrorCode::SourceMissing, source));
            None
        }
        Ok(m) if !m.is_dir() => {
            issues.push(issue(ErrorCode::SourceNotDirectory, source));
            None
        }
        Ok(_) => match std::fs::read_dir(source) {
            Err(_) => {
                issues.push(issue(ErrorCode::SourceUnreadable, source));
                None
            }
            Ok(_) => std::fs::canonicalize(source).ok(),
        },
    };

    let (dest_canon, will_create) = match std::fs::metadata(destination) {
        Ok(m) if !m.is_dir() => {
            issues.push(issue(ErrorCode::DestinationNotDirectory, destination));
            (None, false)
        }
        Ok(_) => {
            let canon = std::fs::canonicalize(destination).ok();
            if !is_writable_dir(destination) {
                issues.push(issue(ErrorCode::DestinationUnwritable, destination));
            }
            (canon, false)
        }
        Err(_) => {
            // Not existing: the nearest parent must exist and be writable.
            match destination.parent().filter(|p| !p.as_os_str().is_empty()) {
                Some(parent) if parent.is_dir() => {
                    if !is_writable_dir(parent) {
                        issues.push(issue(ErrorCode::DestinationUnwritable, parent));
                    }
                    let canon = std::fs::canonicalize(parent)
                        .ok()
                        .and_then(|p| destination.file_name().map(|n| p.join(n)));
                    (canon, true)
                }
                _ => {
                    issues.push(issue(ErrorCode::DestinationParentMissing, destination));
                    (None, true)
                }
            }
        }
    };

    if let (Some(s), Some(d)) = (&source_canon, &dest_canon) {
        if s == d {
            issues.push(issue(ErrorCode::SameSourceAndDestination, destination));
        } else if d.starts_with(s) {
            issues.push(issue(ErrorCode::DestinationInsideSource, destination));
        } else if s.starts_with(d) {
            issues.push(issue(ErrorCode::SourceInsideDestination, source));
        }
    }

    match (source_canon, dest_canon) {
        (Some(source), Some(destination)) if issues.is_empty() => Ok(ValidatedPaths {
            source,
            destination,
            destination_will_be_created: will_create,
        }),
        _ => Err(issues),
    }
}

/// Write test by creating (and removing) a uniquely named probe file. Checks
/// real permissions including ACLs and read-only mounts, unlike mode bits.
fn is_writable_dir(dir: &Path) -> bool {
    let probe = dir.join(format!(
        ".rawimporter-write-probe-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codes(r: Result<ValidatedPaths, Vec<PathIssue>>) -> Vec<ErrorCode> {
        r.err()
            .unwrap_or_default()
            .into_iter()
            .map(|i| i.code)
            .collect()
    }

    #[test]
    fn valid_pair() {
        let t = tempfile::tempdir().unwrap();
        let s = t.path().join("src");
        let d = t.path().join("dst");
        std::fs::create_dir(&s).unwrap();
        std::fs::create_dir(&d).unwrap();
        let v = validate(&s, &d).unwrap();
        assert!(!v.destination_will_be_created);
    }

    #[test]
    fn missing_destination_with_existing_parent_is_ok() {
        let t = tempfile::tempdir().unwrap();
        let s = t.path().join("src");
        std::fs::create_dir(&s).unwrap();
        let v = validate(&s, &t.path().join("new")).unwrap();
        assert!(v.destination_will_be_created);
        assert!(
            !t.path().join("new").exists(),
            "validation must not create folders"
        );
    }

    #[test]
    fn rejects_missing_source_and_missing_parent() {
        let t = tempfile::tempdir().unwrap();
        let c = codes(validate(&t.path().join("nope"), &t.path().join("a/b/c")));
        assert!(c.contains(&ErrorCode::SourceMissing));
        assert!(c.contains(&ErrorCode::DestinationParentMissing));
    }

    #[test]
    fn rejects_identical_and_nested() {
        let t = tempfile::tempdir().unwrap();
        let s = t.path().join("src");
        std::fs::create_dir_all(s.join("inner")).unwrap();
        assert_eq!(
            codes(validate(&s, &s)),
            vec![ErrorCode::SameSourceAndDestination]
        );
        assert_eq!(
            codes(validate(&s, &s.join("inner"))),
            vec![ErrorCode::DestinationInsideSource]
        );
        assert_eq!(
            codes(validate(&s.join("inner"), &s)),
            vec![ErrorCode::SourceInsideDestination]
        );
        // Not-yet-existing destination inside the source is caught as well.
        assert_eq!(
            codes(validate(&s, &s.join("new"))),
            vec![ErrorCode::DestinationInsideSource]
        );
    }

    #[test]
    fn rejects_file_as_directory() {
        let t = tempfile::tempdir().unwrap();
        let f = t.path().join("file");
        std::fs::write(&f, b"x").unwrap();
        let c = codes(validate(&f, &f));
        assert!(c.contains(&ErrorCode::SourceNotDirectory));
        assert!(c.contains(&ErrorCode::DestinationNotDirectory));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_unwritable_destination() {
        use std::os::unix::fs::PermissionsExt;
        if unsafe { libc::geteuid() } == 0 {
            eprintln!("skipped: running as root ignores permissions");
            return;
        }
        let t = tempfile::tempdir().unwrap();
        let s = t.path().join("src");
        let d = t.path().join("dst");
        std::fs::create_dir(&s).unwrap();
        std::fs::create_dir(&d).unwrap();
        std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o555)).unwrap();
        assert_eq!(
            codes(validate(&s, &d)),
            vec![ErrorCode::DestinationUnwritable]
        );
        std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}
