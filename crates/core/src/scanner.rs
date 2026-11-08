//! Streaming directory scan.
//!
//! Differences to 2.x (see FEATURE_PARITY.md §2.1):
//! * symbolic links are never followed (no loops, no escaping the source);
//! * hidden entries are excluded by default as in 2.x, but configurable and
//!   counted instead of silently dropped;
//! * unreadable folders are reported instead of silently ignored;
//! * cancellation is checked for every entry.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::Serialize;
use walkdir::WalkDir;

use crate::error::{CoreError, ErrorCode, Result};
use crate::formats::FormatMatcher;
use crate::state::Control;

#[derive(Debug, Clone, Serialize)]
pub struct ScannedFile {
    pub path: PathBuf,
    /// Directory relative to the source root (empty for files in the root).
    pub rel_dir: PathBuf,
    pub size: u64,
    #[serde(skip)]
    pub modified: Option<SystemTime>,
    #[serde(skip)]
    pub created: Option<SystemTime>,
}

impl ScannedFile {
    pub fn file_name(&self) -> &Path {
        Path::new(self.path.file_name().unwrap_or_default())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SkippedEntry {
    pub path: PathBuf,
    pub code: ErrorCode,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ScanResult {
    pub files: Vec<ScannedFile>,
    pub total_bytes: u64,
    /// Folders or files that could not be read.
    pub unreadable: Vec<SkippedEntry>,
    pub symlinks_skipped: u64,
    /// Hidden files plus hidden folders (each pruned folder counts once).
    pub hidden_skipped: u64,
    /// Files seen that did not match the format filter.
    pub filtered_out: u64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ScanOptions {
    pub include_hidden: bool,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct ScanProgress {
    pub entries_seen: u64,
    pub files_matched: u64,
}

fn is_hidden(name: &std::ffi::OsStr) -> bool {
    name.to_str().map(|n| n.starts_with('.')).unwrap_or(false)
}

/// Scans `root` recursively. `on_progress` is called every 256 entries.
pub fn scan(
    root: &Path,
    matcher: &FormatMatcher,
    opts: ScanOptions,
    control: &Control,
    mut on_progress: impl FnMut(ScanProgress),
) -> Result<ScanResult> {
    let mut result = ScanResult::default();
    let mut seen: u64 = 0;
    let hidden_dirs = std::cell::Cell::new(0u64);

    let walker = WalkDir::new(root)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| {
            let keep = e.depth() == 0 || opts.include_hidden || !is_hidden(e.file_name());
            // Hidden folders are pruned as a whole; count them, not their content.
            if !keep && e.file_type().is_dir() {
                hidden_dirs.set(hidden_dirs.get() + 1);
            }
            keep || !e.file_type().is_dir()
        });

    for entry in walker {
        if control.is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        seen += 1;
        if seen.is_multiple_of(256) {
            on_progress(ScanProgress {
                entries_seen: seen,
                files_matched: result.files.len() as u64,
            });
        }

        let entry = match entry {
            Ok(e) => e,
            Err(err) => {
                let path = err
                    .path()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| root.to_path_buf());
                let code = err
                    .io_error()
                    .map(|io| {
                        CoreError::io(&path, std::io::Error::new(io.kind(), io.to_string())).code()
                    })
                    .unwrap_or(ErrorCode::Io);
                result.unreadable.push(SkippedEntry { path, code });
                continue;
            }
        };

        let ft = entry.file_type();
        if ft.is_symlink() {
            result.symlinks_skipped += 1;
            continue;
        }
        if !ft.is_file() {
            continue;
        }
        // Hidden *files* in the root are not pruned by filter_entry's depth rule.
        if !opts.include_hidden && is_hidden(entry.file_name()) {
            result.hidden_skipped += 1;
            continue;
        }
        if !matcher.matches(entry.path()) {
            result.filtered_out += 1;
            continue;
        }
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => {
                result.unreadable.push(SkippedEntry {
                    path: entry.path().to_path_buf(),
                    code: ErrorCode::SourceUnreadable,
                });
                continue;
            }
        };
        let rel_dir = entry
            .path()
            .parent()
            .and_then(|p| p.strip_prefix(root).ok())
            .map(Path::to_path_buf)
            .unwrap_or_default();
        result.total_bytes += meta.len();
        result.files.push(ScannedFile {
            path: entry.path().to_path_buf(),
            rel_dir,
            size: meta.len(),
            modified: meta.modified().ok(),
            created: meta.created().ok(),
        });
    }

    result.hidden_skipped += hidden_dirs.get();
    on_progress(ScanProgress {
        entries_seen: seen,
        files_matched: result.files.len() as u64,
    });
    Ok(result)
}
