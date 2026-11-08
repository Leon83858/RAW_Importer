//! SQLite-backed import history with transactional session writes.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::copier::{hash_file, DEFAULT_BUFFER};
use crate::error::CoreError;
use crate::platform;
use crate::state::Control;

const SCHEMA_VERSION: i64 = 4;
const SHA256_ALGORITHM_VERSION: i64 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistorySession {
    pub session_id: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub source_path: PathBuf,
    pub destination_path: PathBuf,
    pub status: String,
    pub total_files: u64,
    pub imported_files: u64,
    pub skipped_files: u64,
    pub failed_files: u64,
    pub bytes_total: u64,
    pub bytes_done: u64,
    pub settings_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryFile {
    pub source_path: PathBuf,
    pub destination_path: Option<PathBuf>,
    pub file_size: Option<u64>,
    pub outcome: String,
    pub error_code: Option<String>,
    pub detail: Option<String>,
    pub detection_method: Option<String>,
    pub hash_cache_used: bool,
    pub sha256: Option<[u8; 32]>,
    pub hash_algorithm_version: Option<u32>,
    pub duplicate_status: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HashSource {
    CacheHit,
    Calculated,
    CalculatedUncached,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HashCacheResult {
    pub sha256: [u8; 32],
    pub file_size: u64,
    pub source: HashSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScopedHashMatch {
    pub path: PathBuf,
    pub hash_source: HashSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScopedHashSearchResult {
    pub matches: Vec<ScopedHashMatch>,
    pub candidate_paths_checked: u64,
    pub candidates_hashed: u64,
    pub cached_hashes_used: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    #[error("history database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("history filesystem error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("history database schema version {0} is newer than supported")]
    UnsupportedSchema(i64),
    #[error("history counter exceeds SQLite's signed 64-bit integer range")]
    CounterOverflow,
    #[error("could not serialize import settings for history: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum HashCacheError {
    #[error(transparent)]
    History(#[from] HistoryError),
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error("source changed while hashing: {0}")]
    SourceChanged(PathBuf),
}

pub struct HistoryStore {
    connection: Connection,
}

impl HistoryStore {
    pub fn open(path: &Path) -> Result<Self, HistoryError> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|source| HistoryError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let connection = Connection::open(path)?;
        connection.execute_batch("PRAGMA foreign_keys = ON;")?;
        Self::migrate(connection)
    }

    fn migrate(connection: Connection) -> Result<Self, HistoryError> {
        let mut version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if !(0..=SCHEMA_VERSION).contains(&version) {
            return Err(HistoryError::UnsupportedSchema(version));
        }
        if version == 0 {
            {
                let transaction = connection.unchecked_transaction()?;
                transaction.execute_batch(
                    "CREATE TABLE import_sessions (
                        session_id TEXT PRIMARY KEY NOT NULL,
                        started_at TEXT NOT NULL,
                        ended_at TEXT,
                        source_path TEXT NOT NULL,
                        destination_path TEXT NOT NULL,
                        status TEXT NOT NULL,
                        total_files INTEGER NOT NULL,
                        imported_files INTEGER NOT NULL,
                        skipped_files INTEGER NOT NULL,
                        failed_files INTEGER NOT NULL,
                        bytes_total INTEGER NOT NULL,
                        bytes_done INTEGER NOT NULL,
                        settings_json TEXT NOT NULL
                    );
                    CREATE TABLE import_files (
                        session_id TEXT NOT NULL REFERENCES import_sessions(session_id) ON DELETE CASCADE,
                        source_path TEXT NOT NULL,
                        destination_path TEXT,
                        outcome TEXT NOT NULL,
                        error_code TEXT,
                        detail TEXT,
                        PRIMARY KEY (session_id, source_path)
                    );
                    CREATE INDEX import_sessions_started_at ON import_sessions(started_at DESC);
                    PRAGMA user_version = 1;",
                )?;
                transaction.commit()?;
            }
            version = 1;
        }
        if version == 1 {
            let transaction = connection.unchecked_transaction()?;
            transaction.execute_batch(
                "CREATE TABLE file_hash_cache (
                    canonical_path TEXT PRIMARY KEY NOT NULL,
                    file_size INTEGER NOT NULL,
                    modified_seconds INTEGER NOT NULL,
                    modified_nanos INTEGER NOT NULL,
                    file_identity TEXT NOT NULL,
                    sha256 BLOB NOT NULL CHECK(length(sha256) = 32),
                    verified_at TEXT NOT NULL,
                    algorithm_version INTEGER NOT NULL,
                    cache_valid INTEGER NOT NULL DEFAULT 1
                );
                CREATE INDEX file_hash_cache_lookup
                    ON file_hash_cache(file_size, sha256, cache_valid);
                PRAGMA user_version = 2;",
            )?;
            transaction.commit()?;
            version = 2;
        }
        if version == 2 {
            let transaction = connection.unchecked_transaction()?;
            transaction.execute_batch(
                "ALTER TABLE import_files ADD COLUMN file_size INTEGER;
                ALTER TABLE import_files ADD COLUMN detection_method TEXT;
                ALTER TABLE import_files ADD COLUMN hash_cache_used INTEGER NOT NULL DEFAULT 0;
                ALTER TABLE import_files ADD COLUMN sha256 BLOB
                    CHECK(sha256 IS NULL OR length(sha256) = 32);
                ALTER TABLE import_files ADD COLUMN hash_algorithm_version INTEGER;
                ALTER TABLE import_files ADD COLUMN duplicate_status TEXT;
                CREATE INDEX import_files_sha256 ON import_files(sha256);
                PRAGMA user_version = 3;",
            )?;
            transaction.commit()?;
            version = 3;
        }
        if version == 3 {
            let transaction = connection.unchecked_transaction()?;
            transaction.execute_batch(
                "ALTER TABLE file_hash_cache ADD COLUMN changed_seconds INTEGER;
                ALTER TABLE file_hash_cache ADD COLUMN changed_nanos INTEGER;
                PRAGMA user_version = 4;",
            )?;
            transaction.commit()?;
        }
        Ok(Self { connection })
    }

    /// Writes a session and all its file rows atomically.
    pub fn record_session(
        &mut self,
        session: &HistorySession,
        files: &[HistoryFile],
    ) -> Result<(), HistoryError> {
        let transaction = self.connection.transaction()?;
        transaction.execute(
            "INSERT INTO import_sessions (
                session_id, started_at, ended_at, source_path, destination_path, status,
                total_files, imported_files, skipped_files, failed_files, bytes_total,
                bytes_done, settings_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                session.session_id,
                session.started_at,
                session.ended_at,
                session.source_path.to_string_lossy(),
                session.destination_path.to_string_lossy(),
                session.status,
                to_sql_integer(session.total_files)?,
                to_sql_integer(session.imported_files)?,
                to_sql_integer(session.skipped_files)?,
                to_sql_integer(session.failed_files)?,
                to_sql_integer(session.bytes_total)?,
                to_sql_integer(session.bytes_done)?,
                session.settings_json,
            ],
        )?;
        for file in files {
            transaction.execute(
                "INSERT INTO import_files (
                    session_id, source_path, destination_path, outcome, error_code, detail,
                    file_size, detection_method, hash_cache_used, sha256,
                    hash_algorithm_version, duplicate_status
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    session.session_id,
                    file.source_path.to_string_lossy(),
                    file.destination_path
                        .as_ref()
                        .map(|path| path.to_string_lossy().into_owned()),
                    file.outcome,
                    file.error_code,
                    file.detail,
                    file.file_size.map(to_sql_integer).transpose()?,
                    file.detection_method,
                    i64::from(file.hash_cache_used),
                    file.sha256.map(|digest| digest.to_vec()),
                    file.hash_algorithm_version.map(i64::from),
                    file.duplicate_status,
                ],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn list_sessions(&self, limit: u32) -> Result<Vec<HistorySession>, HistoryError> {
        let mut statement = self.connection.prepare(
            "SELECT session_id, started_at, ended_at, source_path, destination_path, status,
                    total_files, imported_files, skipped_files, failed_files, bytes_total,
                    bytes_done, settings_json
             FROM import_sessions ORDER BY started_at DESC, session_id DESC LIMIT ?1",
        )?;
        let rows = statement.query_map([limit.clamp(1, 1000)], read_session)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(HistoryError::from)
    }

    pub fn get_session(&self, session_id: &str) -> Result<Option<HistorySession>, HistoryError> {
        self.connection
            .query_row(
                "SELECT session_id, started_at, ended_at, source_path, destination_path, status,
                        total_files, imported_files, skipped_files, failed_files, bytes_total,
                        bytes_done, settings_json
                 FROM import_sessions WHERE session_id = ?1",
                [session_id],
                read_session,
            )
            .optional()
            .map_err(HistoryError::from)
    }

    pub fn list_files(&self, session_id: &str) -> Result<Vec<HistoryFile>, HistoryError> {
        let mut statement = self.connection.prepare(
            "SELECT source_path, destination_path, outcome, error_code, detail,
                    file_size, detection_method, hash_cache_used, sha256,
                    hash_algorithm_version, duplicate_status
             FROM import_files WHERE session_id = ?1 ORDER BY source_path",
        )?;
        let rows = statement.query_map([session_id], |row| {
            Ok(HistoryFile {
                source_path: PathBuf::from(row.get::<_, String>(0)?),
                destination_path: row.get::<_, Option<String>>(1)?.map(PathBuf::from),
                outcome: row.get(2)?,
                error_code: row.get(3)?,
                detail: row.get(4)?,
                file_size: row.get::<_, Option<i64>>(5)?.map(|size| size.max(0) as u64),
                detection_method: row.get(6)?,
                hash_cache_used: row.get::<_, i64>(7)? != 0,
                sha256: row
                    .get::<_, Option<Vec<u8>>>(8)?
                    .and_then(|bytes| bytes.try_into().ok()),
                hash_algorithm_version: row
                    .get::<_, Option<i64>>(9)?
                    .map(|version| version.max(0) as u32),
                duplicate_status: row.get(10)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(HistoryError::from)
    }

    /// Returns only destinations recorded as successfully imported.
    pub fn previous_import_paths(&self) -> Result<Vec<PathBuf>, HistoryError> {
        let mut statement = self.connection.prepare(
            "SELECT DISTINCT destination_path FROM import_files
             WHERE outcome = 'imported' AND destination_path IS NOT NULL
             ORDER BY destination_path",
        )?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0).map(PathBuf::from))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(HistoryError::from)
    }

    /// Returns a cached checksum only when file identity and metadata still match.
    /// On platforms without a stable file identity, hashes are recalculated and
    /// deliberately not stored for reuse.
    pub fn hash_file_cached(
        &mut self,
        path: &Path,
        control: &Control,
        force_recalculate: bool,
    ) -> Result<HashCacheResult, HashCacheError> {
        self.hash_file_with_cache_options(path, control, true, force_recalculate)
    }

    pub fn hash_file_with_cache_options(
        &mut self,
        path: &Path,
        control: &Control,
        use_hash_cache: bool,
        force_recalculate: bool,
    ) -> Result<HashCacheResult, HashCacheError> {
        let before = FileSnapshot::read(path)?;
        if use_hash_cache && !force_recalculate && before.is_cacheable() {
            if let Some(sha256) = self.cached_hash(&before)? {
                return Ok(HashCacheResult {
                    sha256,
                    file_size: before.file_size,
                    source: HashSource::CacheHit,
                });
            }
        }

        let sha256 = hash_file(path, DEFAULT_BUFFER, control)?;
        let after = FileSnapshot::read(path)?;
        if before != after {
            return Err(HashCacheError::SourceChanged(path.to_path_buf()));
        }

        if use_hash_cache && before.is_cacheable() {
            self.store_hash(&before, &sha256)?;
            Ok(HashCacheResult {
                sha256,
                file_size: before.file_size,
                source: HashSource::Calculated,
            })
        } else {
            if use_hash_cache {
                self.invalidate_key(&before.canonical_path)?;
            }
            Ok(HashCacheResult {
                sha256,
                file_size: before.file_size,
                source: HashSource::CalculatedUncached,
            })
        }
    }

    /// Compares hashes only for the caller-provided candidate paths.
    /// The caller is responsible for constructing candidates from the selected scope.
    pub fn find_identical_files_in_scope(
        &mut self,
        candidates: &[PathBuf],
        expected_size: u64,
        expected_sha256: &[u8; 32],
        use_hash_cache: bool,
        force_recalculate: bool,
        control: &Control,
    ) -> Result<ScopedHashSearchResult, HashCacheError> {
        let mut matches = Vec::new();
        let mut candidate_paths_checked = 0;
        let mut candidates_hashed = 0;
        let mut cached_hashes_used = 0;
        for path in candidates {
            let metadata = match fs::metadata(path) {
                Ok(metadata) if metadata.is_file() => metadata,
                Ok(_) => continue,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    self.invalidate_path(path)?;
                    continue;
                }
                Err(source) => {
                    return Err(HistoryError::Io {
                        path: path.clone(),
                        source,
                    }
                    .into())
                }
            };
            candidate_paths_checked += 1;
            if metadata.len() != expected_size {
                continue;
            }
            let result = self.hash_file_with_cache_options(
                path,
                control,
                use_hash_cache,
                force_recalculate,
            )?;
            candidates_hashed += 1;
            if result.source == HashSource::CacheHit {
                cached_hashes_used += 1;
            }
            if &result.sha256 == expected_sha256 {
                matches.push(ScopedHashMatch {
                    path: path.clone(),
                    hash_source: result.source,
                });
            }
        }
        Ok(ScopedHashSearchResult {
            matches,
            candidate_paths_checked,
            candidates_hashed,
            cached_hashes_used,
        })
    }

    fn cached_hash(&self, snapshot: &FileSnapshot) -> Result<Option<[u8; 32]>, HistoryError> {
        let Some(fingerprint) = snapshot.cache_fingerprint() else {
            return Ok(None);
        };
        let row: Option<CachedHashRow> = self
            .connection
            .query_row(
                "SELECT file_size, modified_seconds, modified_nanos, changed_seconds,
                    changed_nanos, file_identity, sha256, algorithm_version, cache_valid
                 FROM file_hash_cache WHERE canonical_path = ?1",
                [&snapshot.canonical_path],
                |row| {
                    Ok(CachedHashRow {
                        size: row.get(0)?,
                        seconds: row.get(1)?,
                        nanos: row.get(2)?,
                        changed_seconds: row.get(3)?,
                        changed_nanos: row.get(4)?,
                        identity: row.get(5)?,
                        digest: row.get(6)?,
                        algorithm: row.get(7)?,
                        valid: row.get(8)?,
                    })
                },
            )
            .optional()?;
        let Some(row) = row else {
            return Ok(None);
        };
        if row.valid == 1
            && row.size == fingerprint.file_size
            && row.seconds == fingerprint.modified_seconds
            && row.nanos == fingerprint.modified_nanos
            && row.changed_seconds == Some(fingerprint.changed_seconds)
            && row.changed_nanos == Some(fingerprint.changed_nanos)
            && row.identity == fingerprint.file_identity
            && row.algorithm == SHA256_ALGORITHM_VERSION
            && row.digest.len() == 32
        {
            let mut sha256 = [0; 32];
            sha256.copy_from_slice(&row.digest);
            return Ok(Some(sha256));
        }
        self.invalidate_key(&snapshot.canonical_path)?;
        Ok(None)
    }

    fn store_hash(&self, snapshot: &FileSnapshot, sha256: &[u8; 32]) -> Result<(), HistoryError> {
        let Some(fingerprint) = snapshot.cache_fingerprint() else {
            return Ok(());
        };
        let verified_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Nanos, true);
        self.connection.execute(
            "INSERT INTO file_hash_cache (
                canonical_path, file_size, modified_seconds, modified_nanos,
                changed_seconds, changed_nanos,
                file_identity, sha256, verified_at, algorithm_version, cache_valid
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 1)
            ON CONFLICT(canonical_path) DO UPDATE SET
                file_size = excluded.file_size,
                modified_seconds = excluded.modified_seconds,
                modified_nanos = excluded.modified_nanos,
                changed_seconds = excluded.changed_seconds,
                changed_nanos = excluded.changed_nanos,
                file_identity = excluded.file_identity,
                sha256 = excluded.sha256,
                verified_at = excluded.verified_at,
                algorithm_version = excluded.algorithm_version,
                cache_valid = 1",
            params![
                snapshot.canonical_path,
                fingerprint.file_size,
                fingerprint.modified_seconds,
                fingerprint.modified_nanos,
                fingerprint.changed_seconds,
                fingerprint.changed_nanos,
                fingerprint.file_identity,
                sha256.as_slice(),
                verified_at,
                SHA256_ALGORITHM_VERSION,
            ],
        )?;
        Ok(())
    }

    fn invalidate_path(&self, path: &Path) -> Result<(), HistoryError> {
        let cache_key = fs::canonicalize(path)
            .unwrap_or_else(|_| path.to_path_buf())
            .to_string_lossy()
            .into_owned();
        self.invalidate_key(&cache_key)
    }

    fn invalidate_key(&self, cache_key: &str) -> Result<(), HistoryError> {
        self.connection.execute(
            "UPDATE file_hash_cache SET cache_valid = 0 WHERE canonical_path = ?1",
            [cache_key],
        )?;
        Ok(())
    }
}

struct CachedHashRow {
    size: i64,
    seconds: i64,
    nanos: i64,
    changed_seconds: Option<i64>,
    changed_nanos: Option<i64>,
    identity: String,
    digest: Vec<u8>,
    algorithm: i64,
    valid: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileSnapshot {
    canonical_path: String,
    file_size: u64,
    modified_seconds: Option<i64>,
    modified_nanos: Option<i64>,
    changed_seconds: Option<i64>,
    changed_nanos: Option<i64>,
    file_identity: Option<String>,
}

struct CacheFingerprint {
    file_size: i64,
    modified_seconds: i64,
    modified_nanos: i64,
    changed_seconds: i64,
    changed_nanos: i64,
    file_identity: String,
}

impl FileSnapshot {
    fn read(path: &Path) -> Result<Self, HistoryError> {
        let canonical = fs::canonicalize(path).map_err(|source| HistoryError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let metadata = fs::metadata(&canonical).map_err(|source| HistoryError::Io {
            path: canonical.clone(),
            source,
        })?;
        let modified = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok());
        let (modified_seconds, modified_nanos) = modified
            .map(|time| {
                (
                    i64::try_from(time.as_secs()).ok(),
                    Some(i64::from(time.subsec_nanos())),
                )
            })
            .unwrap_or((None, None));
        #[cfg(unix)]
        let (changed_seconds, changed_nanos) = {
            use std::os::unix::fs::MetadataExt;
            (Some(metadata.ctime()), Some(metadata.ctime_nsec()))
        };
        #[cfg(not(unix))]
        let (changed_seconds, changed_nanos) = (None, None);
        Ok(Self {
            canonical_path: canonical.to_string_lossy().into_owned(),
            file_size: metadata.len(),
            modified_seconds,
            modified_nanos,
            changed_seconds,
            changed_nanos,
            file_identity: platform::stable_file_identity(&metadata),
        })
    }

    fn is_cacheable(&self) -> bool {
        self.cache_fingerprint().is_some()
    }

    fn cache_fingerprint(&self) -> Option<CacheFingerprint> {
        Some(CacheFingerprint {
            file_size: i64::try_from(self.file_size).ok()?,
            modified_seconds: self.modified_seconds?,
            modified_nanos: self.modified_nanos?,
            changed_seconds: self.changed_seconds?,
            changed_nanos: self.changed_nanos?,
            file_identity: self.file_identity.clone()?,
        })
    }
}

fn to_sql_integer(value: u64) -> Result<i64, HistoryError> {
    i64::try_from(value).map_err(|_| HistoryError::CounterOverflow)
}

fn read_session(row: &rusqlite::Row<'_>) -> rusqlite::Result<HistorySession> {
    let integer = |index| -> rusqlite::Result<u64> {
        let value: i64 = row.get(index)?;
        Ok(value.max(0) as u64)
    };
    Ok(HistorySession {
        session_id: row.get(0)?,
        started_at: row.get(1)?,
        ended_at: row.get(2)?,
        source_path: PathBuf::from(row.get::<_, String>(3)?),
        destination_path: PathBuf::from(row.get::<_, String>(4)?),
        status: row.get(5)?,
        total_files: integer(6)?,
        imported_files: integer(7)?,
        skipped_files: integer(8)?,
        failed_files: integer(9)?,
        bytes_total: integer(10)?,
        bytes_done: integer(11)?,
        settings_json: row.get(12)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expected_calculation(source: HashSource, cacheable: bool) {
        assert_eq!(
            source,
            if cacheable {
                HashSource::Calculated
            } else {
                HashSource::CalculatedUncached
            }
        );
    }

    fn session(id: &str) -> HistorySession {
        HistorySession {
            session_id: id.into(),
            started_at: "2026-11-08T10:00:00Z".into(),
            ended_at: Some("2026-11-08T10:01:00Z".into()),
            source_path: "/photos/in".into(),
            destination_path: "/photos/out".into(),
            status: "completed_with_errors".into(),
            total_files: 2,
            imported_files: 1,
            skipped_files: 0,
            failed_files: 1,
            bytes_total: 2048,
            bytes_done: 1024,
            settings_json: "{}".into(),
        }
    }

    #[test]
    fn session_and_file_results_survive_reopening_the_database() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("history.sqlite");
        let mut store = HistoryStore::open(&path).unwrap();
        let file = HistoryFile {
            source_path: "/photos/in/a.arw".into(),
            destination_path: Some("/photos/out/a.arw".into()),
            file_size: Some(2048),
            outcome: "imported".into(),
            error_code: None,
            detail: None,
            detection_method: Some("sha256".into()),
            hash_cache_used: true,
            sha256: Some([7; 32]),
            hash_algorithm_version: Some(1),
            duplicate_status: Some("unique".into()),
        };
        store
            .record_session(&session("one"), std::slice::from_ref(&file))
            .unwrap();
        drop(store);

        let store = HistoryStore::open(&path).unwrap();
        assert_eq!(store.get_session("one").unwrap(), Some(session("one")));
        assert_eq!(store.list_files("one").unwrap(), vec![file]);
        assert_eq!(
            store.previous_import_paths().unwrap(),
            vec![PathBuf::from("/photos/out/a.arw")]
        );
        assert_eq!(store.list_sessions(0).unwrap().len(), 1);
    }

    #[test]
    fn duplicate_session_rolls_back_all_file_rows() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("history.sqlite");
        let mut store = HistoryStore::open(&path).unwrap();
        let file = HistoryFile {
            source_path: "/photos/in/a.arw".into(),
            destination_path: None,
            file_size: None,
            outcome: "failed".into(),
            error_code: Some("io".into()),
            detail: Some("write error".into()),
            detection_method: None,
            hash_cache_used: false,
            sha256: None,
            hash_algorithm_version: None,
            duplicate_status: None,
        };
        store.record_session(&session("one"), &[]).unwrap();
        assert!(store.record_session(&session("one"), &[file]).is_err());
        assert!(store.list_files("one").unwrap().is_empty());
    }

    #[test]
    fn hash_cache_reuses_only_unchanged_files_and_force_rehash_bypasses_it() {
        let temp = tempfile::tempdir().unwrap();
        let db_path = temp.path().join("history.sqlite");
        let source = temp.path().join("first.arw");
        fs::write(&source, b"first content").unwrap();
        let metadata = fs::metadata(&source).unwrap();
        let cacheable = platform::stable_file_identity(&metadata).is_some()
            && metadata
                .modified()
                .is_ok_and(|time| time.duration_since(UNIX_EPOCH).is_ok());
        let mut store = HistoryStore::open(&db_path).unwrap();
        let first = store
            .hash_file_cached(&source, &Control::default(), false)
            .unwrap();
        expected_calculation(first.source, cacheable);

        let reused = store
            .hash_file_cached(&source, &Control::default(), false)
            .unwrap();
        assert_eq!(
            reused.source,
            if cacheable {
                HashSource::CacheHit
            } else {
                HashSource::CalculatedUncached
            }
        );
        assert_eq!(reused.sha256, first.sha256);

        let disabled = store
            .hash_file_with_cache_options(&source, &Control::default(), false, false)
            .unwrap();
        assert_eq!(disabled.source, HashSource::CalculatedUncached);
        assert_eq!(disabled.sha256, first.sha256);
        let cache_still_valid = store
            .hash_file_cached(&source, &Control::default(), false)
            .unwrap();
        assert_eq!(
            cache_still_valid.source,
            if cacheable {
                HashSource::CacheHit
            } else {
                HashSource::CalculatedUncached
            }
        );

        fs::write(&source, b"different and longer content").unwrap();
        let changed = store
            .hash_file_cached(&source, &Control::default(), false)
            .unwrap();
        expected_calculation(changed.source, cacheable);
        assert_ne!(changed.sha256, first.sha256);

        let forced = store
            .hash_file_cached(&source, &Control::default(), true)
            .unwrap();
        expected_calculation(forced.source, cacheable);
        assert_eq!(forced.sha256, changed.sha256);
    }

    #[test]
    fn scoped_hash_search_finds_identical_content_across_names_only_in_scope() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = HistoryStore::open(&temp.path().join("history.sqlite")).unwrap();
        let expected = temp.path().join("expected.arw");
        let identical = temp.path().join("nested/renamed.nef");
        let different = temp.path().join("same-size.jpg");
        let outside_scope = temp.path().join("outside/different-name.dng");
        fs::create_dir_all(identical.parent().unwrap()).unwrap();
        fs::create_dir_all(outside_scope.parent().unwrap()).unwrap();
        fs::write(&expected, b"same-payload").unwrap();
        fs::write(&identical, b"same-payload").unwrap();
        fs::write(&different, b"diff-payload").unwrap();
        fs::write(&outside_scope, b"same-payload").unwrap();
        let expected_hash = hash_file(&expected, DEFAULT_BUFFER, &Control::default()).unwrap();

        let matches = store
            .find_identical_files_in_scope(
                &[identical.clone(), different],
                fs::metadata(&expected).unwrap().len(),
                &expected_hash,
                true,
                false,
                &Control::default(),
            )
            .unwrap();
        assert_eq!(matches.matches.len(), 1);
        assert_eq!(matches.matches[0].path, identical);
        assert!(!matches
            .matches
            .iter()
            .any(|entry| entry.path == outside_scope));
    }

    #[cfg(unix)]
    #[test]
    fn replacement_with_same_size_and_mtime_invalidates_by_file_identity() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = HistoryStore::open(&temp.path().join("history.sqlite")).unwrap();
        let source = temp.path().join("source.arw");
        let old_path = temp.path().join("old.arw");
        let timestamp = filetime::FileTime::from_unix_time(1_700_000_000, 0);
        fs::write(&source, b"AAAA").unwrap();
        filetime::set_file_mtime(&source, timestamp).unwrap();

        let old_result = store
            .hash_file_cached(&source, &Control::default(), false)
            .unwrap();
        assert_eq!(old_result.source, HashSource::Calculated);

        fs::rename(&source, &old_path).unwrap();
        fs::write(&source, b"BBBB").unwrap();
        filetime::set_file_mtime(&source, timestamp).unwrap();
        let new_result = store
            .hash_file_cached(&source, &Control::default(), false)
            .unwrap();
        assert_eq!(new_result.source, HashSource::Calculated);
        assert_ne!(new_result.sha256, old_result.sha256);
    }

    #[cfg(unix)]
    #[test]
    fn in_place_change_with_restored_mtime_invalidates_by_change_time() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = HistoryStore::open(&temp.path().join("history.sqlite")).unwrap();
        let source = temp.path().join("source.arw");
        let timestamp = filetime::FileTime::from_unix_time(1_700_000_000, 0);
        fs::write(&source, b"AAAA").unwrap();
        filetime::set_file_mtime(&source, timestamp).unwrap();
        let first = store
            .hash_file_cached(&source, &Control::default(), false)
            .unwrap();
        let original_metadata = fs::metadata(&source).unwrap();
        let original_identity = platform::stable_file_identity(&original_metadata);
        let ctime_of = |m: &fs::Metadata| {
            use std::os::unix::fs::MetadataExt;
            (m.ctime(), m.ctime_nsec())
        };

        // Linux stamps ctime from a coarse clock (one scheduler tick, up to ~10 ms),
        // so a change made right after the first hash can carry the same ctime.
        // Retry until the kernel reports a different ctime; the code under test is
        // only expected to notice changes the file system makes visible.
        let mut attempts = 0;
        loop {
            fs::write(&source, b"BBBB").unwrap();
            filetime::set_file_mtime(&source, timestamp).unwrap();
            if ctime_of(&fs::metadata(&source).unwrap()) != ctime_of(&original_metadata) {
                break;
            }
            attempts += 1;
            assert!(attempts < 200, "ctime never changed on this file system");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let changed_identity = platform::stable_file_identity(&fs::metadata(&source).unwrap());
        assert_eq!(original_identity, changed_identity);

        let changed = store
            .hash_file_cached(&source, &Control::default(), false)
            .unwrap();
        assert_eq!(changed.source, HashSource::Calculated);
        assert_ne!(changed.sha256, first.sha256);
    }

    #[test]
    fn cancelled_hash_does_not_create_a_cache_entry() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = HistoryStore::open(&temp.path().join("history.sqlite")).unwrap();
        let source = temp.path().join("cancelled.arw");
        fs::write(&source, vec![7; 1024 * 1024]).unwrap();
        let control = Control::default();
        control.cancel();

        assert!(matches!(
            store.hash_file_cached(&source, &control, false),
            Err(HashCacheError::Core(CoreError::Cancelled))
        ));
        let count: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM file_hash_cache", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn schema_one_is_upgraded_without_dropping_history_rows() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("history.sqlite");
        let mut store = HistoryStore::open(&path).unwrap();
        store
            .record_session(&session("before-upgrade"), &[])
            .unwrap();
        store
            .connection
            .execute_batch(
                "DROP TABLE file_hash_cache;
                 DROP INDEX import_files_sha256;
                 ALTER TABLE import_files DROP COLUMN duplicate_status;
                 ALTER TABLE import_files DROP COLUMN hash_algorithm_version;
                 ALTER TABLE import_files DROP COLUMN sha256;
                 ALTER TABLE import_files DROP COLUMN hash_cache_used;
                 ALTER TABLE import_files DROP COLUMN detection_method;
                 ALTER TABLE import_files DROP COLUMN file_size;
                 PRAGMA user_version = 1;",
            )
            .unwrap();
        drop(store);

        let store = HistoryStore::open(&path).unwrap();
        assert_eq!(
            store.get_session("before-upgrade").unwrap(),
            Some(session("before-upgrade"))
        );
        let version: i64 = store
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
    }

    #[cfg(unix)]
    #[test]
    fn schema_three_cache_entries_are_not_reused_after_ctime_migration() {
        let temp = tempfile::tempdir().unwrap();
        let database = temp.path().join("history.sqlite");
        let source = temp.path().join("source.arw");
        fs::write(&source, b"payload").unwrap();
        let mut store = HistoryStore::open(&database).unwrap();
        assert_eq!(
            store
                .hash_file_cached(&source, &Control::default(), false)
                .unwrap()
                .source,
            HashSource::Calculated
        );
        store
            .connection
            .execute_batch(
                "ALTER TABLE file_hash_cache DROP COLUMN changed_nanos;
                 ALTER TABLE file_hash_cache DROP COLUMN changed_seconds;
                 PRAGMA user_version = 3;",
            )
            .unwrap();
        drop(store);

        let mut upgraded = HistoryStore::open(&database).unwrap();
        let result = upgraded
            .hash_file_cached(&source, &Control::default(), false)
            .unwrap();
        assert_eq!(result.source, HashSource::Calculated);
        assert_eq!(
            result.sha256,
            hash_file(&source, DEFAULT_BUFFER, &Control::default()).unwrap()
        );
    }

    #[test]
    fn rejects_a_database_created_by_a_newer_schema() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("history.sqlite");
        let connection = Connection::open(&path).unwrap();
        connection
            .pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .unwrap();
        drop(connection);
        assert!(matches!(
            HistoryStore::open(&path),
            Err(HistoryError::UnsupportedSchema(5))
        ));
    }
}
