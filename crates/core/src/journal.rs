//! Append-only import journal and non-destructive recovery discovery.
//!
//! Recovery only reports unfinished operations and attributable temp files;
//! it never removes or replaces user data.

use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::copier::TEMP_MARKER;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum JournalEvent {
    SessionStarted {
        session_id: String,
        session_tag: String,
        destination_root: PathBuf,
        started_at: String,
    },
    FileStarted {
        session_id: String,
        source_path: PathBuf,
        destination_path: PathBuf,
    },
    FileFinished {
        session_id: String,
        source_path: PathBuf,
    },
    SessionFinished {
        session_id: String,
        status: String,
        ended_at: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingFile {
    pub source_path: PathBuf,
    pub destination_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoverableSession {
    pub session_id: String,
    pub session_tag: String,
    pub destination_root: PathBuf,
    pub pending_files: Vec<PendingFile>,
    pub temp_files: Vec<PathBuf>,
}

#[derive(Debug, thiserror::Error)]
pub enum JournalError {
    #[error("journal I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("journal line {line} is invalid JSON: {source}")]
    Json {
        line: usize,
        #[source]
        source: serde_json::Error,
    },
}

pub struct Journal {
    path: PathBuf,
    writer: Mutex<File>,
}

impl Journal {
    pub fn open(path: &Path) -> Result<Self, JournalError> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent).map_err(|source| JournalError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let writer = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|source| JournalError::Io {
                path: path.to_path_buf(),
                source,
            })?;
        Ok(Self {
            path: path.to_path_buf(),
            writer: Mutex::new(writer),
        })
    }

    pub fn append(&self, event: &JournalEvent) -> Result<(), JournalError> {
        let bytes = serde_json::to_vec(event).expect("journal events are serializable");
        let mut writer = self.writer.lock().unwrap_or_else(|e| e.into_inner());
        writer
            .write_all(&bytes)
            .and_then(|_| writer.write_all(b"\n"))
            .and_then(|_| writer.sync_data())
            .map_err(|source| JournalError::Io {
                path: self.path.clone(),
                source,
            })
    }

    /// Returns unfinished sessions and only temp files attributable by name.
    pub fn recoverable_sessions(&self) -> Result<Vec<RecoverableSession>, JournalError> {
        let file = File::open(&self.path).map_err(|source| JournalError::Io {
            path: self.path.clone(),
            source,
        })?;
        let mut states: HashMap<String, SessionState> = HashMap::new();
        for (index, line) in BufReader::new(file).lines().enumerate() {
            let line = line.map_err(|source| JournalError::Io {
                path: self.path.clone(),
                source,
            })?;
            if line.trim().is_empty() {
                continue;
            }
            let event: JournalEvent =
                serde_json::from_str(&line).map_err(|source| JournalError::Json {
                    line: index + 1,
                    source,
                })?;
            match event {
                JournalEvent::SessionStarted {
                    session_id,
                    session_tag,
                    destination_root,
                    ..
                } => {
                    states.insert(
                        session_id,
                        SessionState {
                            session_tag,
                            destination_root,
                            pending: BTreeMap::new(),
                            finished: false,
                        },
                    );
                }
                JournalEvent::FileStarted {
                    session_id,
                    source_path,
                    destination_path,
                } => {
                    if let Some(state) = states.get_mut(&session_id) {
                        state.pending.insert(source_path, destination_path);
                    }
                }
                JournalEvent::FileFinished {
                    session_id,
                    source_path,
                } => {
                    if let Some(state) = states.get_mut(&session_id) {
                        state.pending.remove(&source_path);
                    }
                }
                JournalEvent::SessionFinished { session_id, .. } => {
                    if let Some(state) = states.get_mut(&session_id) {
                        state.finished = true;
                    }
                }
            }
        }

        let mut recoverable = Vec::new();
        for (session_id, state) in states {
            if state.finished {
                continue;
            }
            let pending_files: Vec<_> = state
                .pending
                .into_iter()
                .map(|(source_path, destination_path)| PendingFile {
                    source_path,
                    destination_path,
                })
                .collect();
            let mut temp_files = Vec::new();
            for pending in &pending_files {
                temp_files.extend(find_temp_files(
                    &pending.destination_path,
                    &state.session_tag,
                )?);
            }
            temp_files.sort();
            temp_files.dedup();
            recoverable.push(RecoverableSession {
                session_id,
                session_tag: state.session_tag,
                destination_root: state.destination_root,
                pending_files,
                temp_files,
            });
        }
        recoverable.sort_by(|a, b| a.session_id.cmp(&b.session_id));
        Ok(recoverable)
    }
}

struct SessionState {
    session_tag: String,
    destination_root: PathBuf,
    pending: BTreeMap<PathBuf, PathBuf>,
    finished: bool,
}

fn find_temp_files(destination: &Path, session_tag: &str) -> Result<Vec<PathBuf>, JournalError> {
    let Some(parent) = destination.parent() else {
        return Ok(Vec::new());
    };
    let name = destination
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_default();
    let prefix = format!(".{name}{TEMP_MARKER}{session_tag}-");
    let entries = match fs::read_dir(parent) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(JournalError::Io {
                path: parent.to_path_buf(),
                source,
            })
        }
    };
    let mut found = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| JournalError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
        if entry
            .file_type()
            .map(|kind| kind.is_file())
            .unwrap_or(false)
            && entry.file_name().to_string_lossy().starts_with(&prefix)
        {
            found.push(entry.path());
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn start(session_id: &str, tag: &str, destination_root: &Path) -> JournalEvent {
        JournalEvent::SessionStarted {
            session_id: session_id.into(),
            session_tag: tag.into(),
            destination_root: destination_root.to_path_buf(),
            started_at: "2026-11-08T10:00:00Z".into(),
        }
    }

    #[test]
    fn discovers_only_attributable_incomplete_temp_files_without_deleting_them() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("destination");
        let target_dir = root.join("2026/11 November/08");
        fs::create_dir_all(&target_dir).unwrap();
        let target = target_dir.join("a.arw");
        let own_temp = target_dir.join(".a.arw.rawimp-session-1-a.part");
        let other_temp = target_dir.join(".a.arw.rawimp-other-2-b.part");
        fs::write(&own_temp, b"partial").unwrap();
        fs::write(&other_temp, b"unrelated").unwrap();

        let journal = Journal::open(&temp.path().join("recovery.jsonl")).unwrap();
        journal
            .append(&start("session-1", "session-1", &root))
            .unwrap();
        journal
            .append(&JournalEvent::FileStarted {
                session_id: "session-1".into(),
                source_path: "/source/a.arw".into(),
                destination_path: target,
            })
            .unwrap();

        let recoverable = journal.recoverable_sessions().unwrap();
        assert_eq!(recoverable.len(), 1);
        assert_eq!(recoverable[0].pending_files.len(), 1);
        assert_eq!(recoverable[0].temp_files, vec![own_temp.clone()]);
        assert!(own_temp.exists());
        assert!(other_temp.exists());
    }

    #[test]
    fn completed_sessions_are_not_reported_for_recovery() {
        let temp = tempfile::tempdir().unwrap();
        let journal = Journal::open(&temp.path().join("recovery.jsonl")).unwrap();
        journal.append(&start("done", "done", temp.path())).unwrap();
        journal
            .append(&JournalEvent::SessionFinished {
                session_id: "done".into(),
                status: "completed".into(),
                ended_at: "2026-11-08T10:01:00Z".into(),
            })
            .unwrap();
        assert!(journal.recoverable_sessions().unwrap().is_empty());
    }

    #[test]
    fn malformed_journal_line_is_reported() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("recovery.jsonl");
        fs::write(&path, b"not json\n").unwrap();
        let journal = Journal::open(&path).unwrap();
        assert!(matches!(
            journal.recoverable_sessions(),
            Err(JournalError::Json { line: 1, .. })
        ));
    }
}
