//! Duplicate / conflict handling (master prompt §11).
//!
//! * `Skip` – 2.x default: skip if the planned destination path exists.
//! * `Rename` – 2.x alternative: `name (1).ext`, `name (2).ext`, …
//! * `SkipIdentical` – new: a file whose *content* (size + SHA-256) equals an
//!   existing file at the planned name or one of its `(n)` variants is
//!   skipped; otherwise the next free name is used. Equal size alone is never
//!   treated as identical.
//!
//! Replacing existing files is intentionally not implemented in the core yet
//! (needs an explicit per-run user confirmation, see IMPLEMENTATION_PLAN.md).

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::copier::hash_file;
use crate::error::{CoreError, Result};
use crate::history::{HashCacheError, HashCacheResult, HistoryStore, ScopedHashMatch};
use crate::settings::DuplicateSettings;
use crate::state::Control;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum ConflictPolicy {
    #[default]
    Skip,
    Rename,
    SkipIdentical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum DuplicateScope {
    DestinationDirectory,
    DestinationTree,
    PreviousImports,
    CurrentSession,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContentDuplicateReport {
    pub source_hash: HashCacheResult,
    pub matches: Vec<ScopedHashMatch>,
    pub candidate_paths_checked: u64,
    pub candidate_files_hashed: u64,
    pub cached_hashes_used: u64,
}

pub struct DuplicateSearchContext<'a> {
    pub planned_destination_dir: &'a Path,
    pub destination_root: &'a Path,
    pub current_session_files: &'a [PathBuf],
    pub previous_imports: &'a [PathBuf],
}

#[derive(Debug, thiserror::Error)]
pub enum ContentDuplicateError {
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error(transparent)]
    Cache(#[from] HashCacheError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum SkipReason {
    DestinationExists,
    IdenticalContent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Copy to this path. The path is reserved until [`Reservations::release`].
    Copy(PathBuf),
    Skip {
        reason: SkipReason,
        existing: PathBuf,
    },
}

/// Upper bound of `(n)` suffixes before a session-tagged fallback is used
/// (2.x: 1000, then a GUID).
pub const MAX_RENAME_ATTEMPTS: u32 = 1000;

/// Final paths currently being written by any worker of this run. Prevents two
/// workers from choosing the same name (2.x relied on retries after the fact).
#[derive(Debug, Default)]
pub struct Reservations(Mutex<HashSet<PathBuf>>);

impl Reservations {
    fn try_reserve(&self, p: &Path) -> bool {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(p.to_path_buf())
    }
    fn is_reserved(&self, p: &Path) -> bool {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).contains(p)
    }
    pub fn release(&self, p: &Path) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).remove(p);
    }
}

/// Collects only candidates belonging to the caller-selected search scopes.
pub fn collect_scope_candidates(
    scopes: &BTreeSet<DuplicateScope>,
    planned_destination_dir: &Path,
    destination_root: &Path,
    current_session_files: &[PathBuf],
    previous_imports: &[PathBuf],
) -> Result<Vec<PathBuf>> {
    let mut candidates = BTreeSet::new();
    for scope in scopes {
        match scope {
            DuplicateScope::DestinationDirectory => {
                collect_directory(planned_destination_dir, &mut candidates)?;
            }
            DuplicateScope::DestinationTree => {
                collect_tree(destination_root, &mut candidates)?;
            }
            DuplicateScope::PreviousImports => {
                candidates.extend(previous_imports.iter().cloned());
            }
            DuplicateScope::CurrentSession => {
                candidates.extend(current_session_files.iter().cloned());
            }
        }
    }
    Ok(candidates.into_iter().collect())
}

fn collect_directory(root: &Path, candidates: &mut BTreeSet<PathBuf>) -> Result<()> {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(CoreError::io(root, error)),
    };
    for entry in entries {
        let entry = entry.map_err(|error| CoreError::io(root, error))?;
        if entry
            .file_type()
            .map_err(|error| CoreError::io(entry.path(), error))?
            .is_file()
        {
            candidates.insert(entry.path());
        }
    }
    Ok(())
}

fn collect_tree(root: &Path, candidates: &mut BTreeSet<PathBuf>) -> Result<()> {
    if !root.exists() {
        return Ok(());
    }
    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry.map_err(|error| {
            let path = error
                .path()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| root.to_path_buf());
            let source = error
                .into_io_error()
                .unwrap_or_else(|| std::io::Error::other("directory traversal failed"));
            CoreError::io(path, source)
        })?;
        if entry.file_type().is_file() {
            candidates.insert(entry.path().to_path_buf());
        }
    }
    Ok(())
}

/// Hashes one source and compares it with files from an explicit candidate list.
/// A storage/hash error is returned; callers must not silently treat it as "no duplicates".
pub fn find_content_duplicates(
    history: &mut HistoryStore,
    source: &Path,
    candidates: &[PathBuf],
    use_hash_cache: bool,
    force_recalculate: bool,
    control: &Control,
) -> Result<ContentDuplicateReport, ContentDuplicateError> {
    let source_path =
        std::fs::canonicalize(source).map_err(|error| CoreError::io(source, error))?;
    let source_hash =
        history.hash_file_with_cache_options(source, control, use_hash_cache, force_recalculate)?;
    let search = history.find_identical_files_in_scope(
        candidates,
        source_hash.file_size,
        &source_hash.sha256,
        use_hash_cache,
        force_recalculate,
        control,
    )?;
    let mut matches = search.matches;
    let cached_hashes_used = search.cached_hashes_used
        + u64::from(source_hash.source == crate::history::HashSource::CacheHit);
    let candidate_paths_checked = search.candidate_paths_checked;
    let candidate_files_hashed = search.candidates_hashed;
    let mut distinct_matches = Vec::with_capacity(matches.len());
    for candidate in matches.drain(..) {
        let candidate_path = std::fs::canonicalize(&candidate.path)
            .map_err(|error| CoreError::io(&candidate.path, error))?;
        if candidate_path != source_path {
            distinct_matches.push(candidate);
        }
    }
    Ok(ContentDuplicateReport {
        source_hash,
        matches: distinct_matches,
        candidate_paths_checked,
        candidate_files_hashed,
        cached_hashes_used,
    })
}

/// Applies the user's enabled/scoped content-detection settings to an import file.
pub fn detect_content_duplicates(
    history: &mut HistoryStore,
    settings: &DuplicateSettings,
    source: &Path,
    search: &DuplicateSearchContext<'_>,
    control: &Control,
) -> Result<Option<ContentDuplicateReport>, ContentDuplicateError> {
    if !settings.content_detection_enabled || settings.scopes.is_empty() {
        return Ok(None);
    }
    let candidates = collect_scope_candidates(
        &settings.scopes,
        search.planned_destination_dir,
        search.destination_root,
        search.current_session_files,
        search.previous_imports,
    )?;
    find_content_duplicates(
        history,
        source,
        &candidates,
        settings.use_hash_cache,
        settings.force_rehash,
        control,
    )
    .map(Some)
}

/// `name.ext` → `name (n).ext`; `n == 0` returns the original name.
pub fn numbered_name(file_name: &Path, n: u32) -> PathBuf {
    if n == 0 {
        return file_name.to_path_buf();
    }
    let stem = file_name
        .file_stem()
        .map(|s| s.to_string_lossy())
        .unwrap_or_default();
    match file_name.extension() {
        Some(ext) => PathBuf::from(format!("{stem} ({n}).{}", ext.to_string_lossy())),
        None => PathBuf::from(format!("{stem} ({n})")),
    }
}

fn exists(p: &Path) -> bool {
    p.symlink_metadata().is_ok()
}

fn same_content(src: &Path, src_size: u64, existing: &Path, control: &Control) -> Result<bool> {
    let meta = match std::fs::metadata(existing) {
        Ok(m) if m.is_file() => m,
        _ => return Ok(false),
    };
    if meta.len() != src_size {
        return Ok(false);
    }
    const BUF: usize = 1024 * 1024;
    Ok(hash_file(src, BUF, control)? == hash_file(existing, BUF, control)?)
}

/// The file that is about to be imported.
#[derive(Debug, Clone, Copy)]
pub struct Incoming<'a> {
    pub path: &'a Path,
    pub size: u64,
    /// File name to use at the destination.
    pub file_name: &'a Path,
}

/// Decides what to do with `incoming` whose planned destination is
/// `dir/incoming.file_name`.
pub fn decide(
    incoming: Incoming<'_>,
    dir: &Path,
    policy: ConflictPolicy,
    reservations: &Reservations,
    session_tag: &str,
    control: &Control,
) -> Result<Decision> {
    let Incoming {
        path: src,
        size: src_size,
        file_name,
    } = incoming;
    let first = dir.join(file_name);
    if policy == ConflictPolicy::Skip {
        if exists(&first) || !reservations.try_reserve(&first) {
            return Ok(Decision::Skip {
                reason: SkipReason::DestinationExists,
                existing: first,
            });
        }
        return Ok(Decision::Copy(first));
    }

    for n in 0..=MAX_RENAME_ATTEMPTS {
        let candidate = dir.join(numbered_name(file_name, n));
        if exists(&candidate) {
            if policy == ConflictPolicy::SkipIdentical
                && same_content(src, src_size, &candidate, control)?
            {
                return Ok(Decision::Skip {
                    reason: SkipReason::IdenticalContent,
                    existing: candidate,
                });
            }
            continue;
        }
        if reservations.is_reserved(&candidate) {
            continue;
        }
        if reservations.try_reserve(&candidate) {
            return Ok(Decision::Copy(candidate));
        }
    }
    // Fallback after MAX_RENAME_ATTEMPTS, analogous to the 2.x GUID suffix.
    let stem = file_name
        .file_stem()
        .map(|s| s.to_string_lossy())
        .unwrap_or_default();
    let ext = file_name
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    for k in 0..u32::MAX {
        let candidate = dir.join(format!("{stem}_{session_tag}_{k}{ext}"));
        if !exists(&candidate) && reservations.try_reserve(&candidate) {
            return Ok(Decision::Copy(candidate));
        }
    }
    Err(CoreError::DestinationExists(first))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::{HistoryFile, HistorySession};

    fn run(policy: ConflictPolicy, src: &Path, dir: &Path, res: &Reservations) -> Decision {
        let size = std::fs::metadata(src).unwrap().len();
        decide(
            Incoming {
                path: src,
                size,
                file_name: Path::new("a.arw"),
            },
            dir,
            policy,
            res,
            "t",
            &Control::default(),
        )
        .unwrap()
    }

    #[test]
    fn numbered_names_match_2x() {
        assert_eq!(
            numbered_name(Path::new("a.arw"), 1),
            PathBuf::from("a (1).arw")
        );
        assert_eq!(
            numbered_name(Path::new("a.tar.gz"), 2),
            PathBuf::from("a.tar (2).gz")
        );
        assert_eq!(
            numbered_name(Path::new("README"), 3),
            PathBuf::from("README (3)")
        );
    }

    #[test]
    fn skip_policy() {
        let t = tempfile::tempdir().unwrap();
        let src = t.path().join("src.arw");
        std::fs::write(&src, b"x").unwrap();
        let dir = t.path().join("d");
        std::fs::create_dir(&dir).unwrap();
        let res = Reservations::default();
        assert_eq!(
            run(ConflictPolicy::Skip, &src, &dir, &res),
            Decision::Copy(dir.join("a.arw"))
        );
        // Same name reserved by another worker -> skipped like 2.x.
        assert!(matches!(
            run(ConflictPolicy::Skip, &src, &dir, &res),
            Decision::Skip { .. }
        ));
        std::fs::write(dir.join("a.arw"), b"y").unwrap();
        res.release(&dir.join("a.arw"));
        assert!(matches!(
            run(ConflictPolicy::Skip, &src, &dir, &res),
            Decision::Skip {
                reason: SkipReason::DestinationExists,
                ..
            }
        ));
    }

    #[test]
    fn rename_policy_uses_next_free_and_unreserved_name() {
        let t = tempfile::tempdir().unwrap();
        let src = t.path().join("src.arw");
        std::fs::write(&src, b"x").unwrap();
        let dir = t.path().to_path_buf();
        std::fs::write(dir.join("a.arw"), b"x").unwrap();
        std::fs::write(dir.join("a (1).arw"), b"x").unwrap();
        let res = Reservations::default();
        assert_eq!(
            run(ConflictPolicy::Rename, &src, &dir, &res),
            Decision::Copy(dir.join("a (2).arw"))
        );
        assert_eq!(
            run(ConflictPolicy::Rename, &src, &dir, &res),
            Decision::Copy(dir.join("a (3).arw"))
        );
    }

    #[test]
    fn skip_identical_compares_content_not_size() {
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().join("d");
        std::fs::create_dir(&dir).unwrap();
        let src = t.path().join("src.arw");
        std::fs::write(&src, b"AAAA").unwrap();
        // Same size, different content at the original name.
        std::fs::write(dir.join("a.arw"), b"BBBB").unwrap();
        let res = Reservations::default();
        assert_eq!(
            run(ConflictPolicy::SkipIdentical, &src, &dir, &res),
            Decision::Copy(dir.join("a (1).arw"))
        );
        res.release(&dir.join("a (1).arw"));
        // Identical content at a numbered variant is recognised (re-import).
        std::fs::write(dir.join("a (1).arw"), b"AAAA").unwrap();
        assert!(matches!(
            run(ConflictPolicy::SkipIdentical, &src, &dir, &res),
            Decision::Skip {
                reason: SkipReason::IdenticalContent,
                ..
            }
        ));
    }

    #[test]
    fn destination_scopes_return_only_files_inside_selected_locations() {
        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("destination");
        let planned = destination.join("2026/11 November/08");
        let session_file = temp.path().join("source/from-this-session.nef");
        let previous_file = temp.path().join("prior-imports/older.jpg");
        let outside_file = temp.path().join("unselected/outside.dng");
        std::fs::create_dir_all(planned.join("nested")).unwrap();
        std::fs::create_dir_all(session_file.parent().unwrap()).unwrap();
        std::fs::create_dir_all(previous_file.parent().unwrap()).unwrap();
        std::fs::create_dir_all(outside_file.parent().unwrap()).unwrap();
        std::fs::write(planned.join("direct.arw"), b"a").unwrap();
        std::fs::write(planned.join("nested/child.arw"), b"b").unwrap();
        std::fs::write(&session_file, b"c").unwrap();
        std::fs::write(&previous_file, b"d").unwrap();
        std::fs::write(&outside_file, b"e").unwrap();

        let direct = collect_scope_candidates(
            &[DuplicateScope::DestinationDirectory].into_iter().collect(),
            &planned,
            &destination,
            std::slice::from_ref(&session_file),
            std::slice::from_ref(&previous_file),
        )
        .unwrap();
        assert_eq!(direct, vec![planned.join("direct.arw")]);

        let tree = collect_scope_candidates(
            &[DuplicateScope::DestinationTree].into_iter().collect(),
            &planned,
            &destination,
            &[],
            &[],
        )
        .unwrap();
        assert_eq!(
            tree,
            vec![planned.join("direct.arw"), planned.join("nested/child.arw")]
        );

        let selected = collect_scope_candidates(
            &[
                DuplicateScope::PreviousImports,
                DuplicateScope::CurrentSession,
            ]
            .into_iter()
            .collect(),
            &planned,
            &destination,
            std::slice::from_ref(&session_file),
            std::slice::from_ref(&previous_file),
        )
        .unwrap();
        assert_eq!(selected, vec![previous_file, session_file]);
        assert!(!selected.contains(&outside_file));
    }

    #[test]
    fn content_check_finds_same_bytes_under_a_different_name() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source/a.arw");
        let identical = temp.path().join("destination/renamed.nef");
        let same_size = temp.path().join("destination/different.jpg");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::create_dir_all(identical.parent().unwrap()).unwrap();
        std::fs::write(&source, b"same-payload").unwrap();
        std::fs::write(&identical, b"same-payload").unwrap();
        std::fs::write(&same_size, b"diff-payload").unwrap();
        let mut history = HistoryStore::open(&temp.path().join("history.sqlite")).unwrap();

        let report = find_content_duplicates(
            &mut history,
            &source,
            &[source.clone(), identical.clone(), same_size],
            true,
            false,
            &Control::default(),
        )
        .unwrap();

        assert_eq!(report.matches.len(), 1);
        assert_eq!(report.matches[0].path, identical);
        assert!(!report.matches[0].path.ends_with("a.arw"));
    }

    #[test]
    fn previous_import_scope_detects_content_across_sessions_and_names() {
        let temp = tempfile::tempdir().unwrap();
        let previous_path = temp.path().join("archive/old-name.arw");
        let incoming_path = temp.path().join("new-card/new-name.nef");
        std::fs::create_dir_all(previous_path.parent().unwrap()).unwrap();
        std::fs::create_dir_all(incoming_path.parent().unwrap()).unwrap();
        std::fs::write(&previous_path, b"same bytes").unwrap();
        std::fs::write(&incoming_path, b"same bytes").unwrap();

        let mut history = HistoryStore::open(&temp.path().join("history.sqlite")).unwrap();
        history
            .record_session(
                &HistorySession {
                    session_id: "prior-session".into(),
                    started_at: "2026-11-08T10:00:00Z".into(),
                    ended_at: Some("2026-11-08T10:01:00Z".into()),
                    source_path: temp.path().join("old-card"),
                    destination_path: temp.path().join("archive"),
                    status: "completed".into(),
                    total_files: 1,
                    imported_files: 1,
                    skipped_files: 0,
                    failed_files: 0,
                    bytes_total: 10,
                    bytes_done: 10,
                    settings_json: "{}".into(),
                },
                &[HistoryFile {
                    source_path: temp.path().join("old-card/original.arw"),
                    destination_path: Some(previous_path.clone()),
                    file_size: Some(10),
                    outcome: "imported".into(),
                    error_code: None,
                    detail: None,
                    detection_method: None,
                    hash_cache_used: false,
                    sha256: None,
                    hash_algorithm_version: None,
                    duplicate_status: None,
                }],
            )
            .unwrap();

        let previous_imports = history.previous_import_paths().unwrap();
        let candidates = collect_scope_candidates(
            &[DuplicateScope::PreviousImports].into_iter().collect(),
            &temp.path().join("next-destination"),
            &temp.path().join("next-destination"),
            &[],
            &previous_imports,
        )
        .unwrap();
        let report = find_content_duplicates(
            &mut history,
            &incoming_path,
            &candidates,
            true,
            false,
            &Control::default(),
        )
        .unwrap();

        assert_eq!(report.matches.len(), 1);
        assert_eq!(report.matches[0].path, previous_path);
    }

    #[test]
    fn duplicate_settings_gate_hashing_and_cache_reuse() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source/a.arw");
        let candidate = temp.path().join("session/renamed.nef");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::create_dir_all(candidate.parent().unwrap()).unwrap();
        std::fs::write(&source, b"same bytes").unwrap();
        std::fs::write(&candidate, b"same bytes").unwrap();
        let mut history = HistoryStore::open(&temp.path().join("history.sqlite")).unwrap();

        assert!(detect_content_duplicates(
            &mut history,
            &DuplicateSettings::default(),
            &source,
            &DuplicateSearchContext {
                planned_destination_dir: candidate.parent().unwrap(),
                destination_root: temp.path(),
                current_session_files: std::slice::from_ref(&candidate),
                previous_imports: &[],
            },
            &Control::default(),
        )
        .unwrap()
        .is_none());

        let settings = DuplicateSettings {
            content_detection_enabled: true,
            scopes: [DuplicateScope::CurrentSession].into_iter().collect(),
            use_hash_cache: false,
            force_rehash: false,
        };
        let report = detect_content_duplicates(
            &mut history,
            &settings,
            &source,
            &DuplicateSearchContext {
                planned_destination_dir: candidate.parent().unwrap(),
                destination_root: temp.path(),
                current_session_files: std::slice::from_ref(&candidate),
                previous_imports: &[],
            },
            &Control::default(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(report.matches.len(), 1);
        assert_eq!(
            report.source_hash.source,
            crate::history::HashSource::CalculatedUncached
        );
        assert_eq!(
            report.matches[0].hash_source,
            crate::history::HashSource::CalculatedUncached
        );
    }
}
