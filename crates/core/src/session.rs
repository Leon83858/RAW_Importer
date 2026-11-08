//! Import session: the single authority for the state of one import window.
//!
//! Drives the [`crate::state`] machine through scan → preview → import, runs
//! the import on a background thread, keeps a progress snapshot, writes the
//! recovery journal and the history. The GUI only calls these methods and
//! renders [`SessionSnapshot`]s; it never decides on its own that an import
//! has finished (master prompt §6).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::copier::CopyOptions;
use crate::engine::{
    planned_dir, run_import_with_history, EngineEvent, ImportJob, ImportRunError,
    ImportSessionContext, Outcome, RunEnd, RunSummary,
};
use crate::error::{CoreError, ErrorCode};
use crate::history::HistoryStore;
use crate::journal::{Journal, JournalEvent};
use crate::metadata::resolve_date;
use crate::metadata::FileTimes;
use crate::paths::{self, PathIssue};
use crate::scanner::{self, ScanOptions, ScannedFile};
use crate::settings::Settings;
use crate::state::{transition, Command, Control, ImportState};

/// Minimum interval between two progress notifications to the GUI.
pub const NOTIFY_INTERVAL: Duration = Duration::from_millis(100);

/// Where the session keeps its persistent data (OS-specific app data dir).
#[derive(Debug, Clone)]
pub struct SessionPaths {
    pub history_db: PathBuf,
    pub journal: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct PreviewFile {
    pub source: PathBuf,
    /// Planned destination directory relative to the destination root;
    /// `None` if no date could be determined (file will fail, not guess).
    pub relative_destination: Option<PathBuf>,
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub size: u64,
    /// `true` if the date came from a fallback source of the policy.
    pub date_fallback: bool,
    pub date_source: Option<crate::metadata::DateSource>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct ScanPreview {
    pub files: Vec<PreviewFile>,
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub total_bytes: u64,
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub filtered_out: u64,
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub hidden_skipped: u64,
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub symlinks_skipped: u64,
    pub unreadable: Vec<PathIssue>,
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub missing_date_count: u64,
    pub destination_will_be_created: bool,
}

/// Live progress. Speeds and ETA are averages over the whole run so far
/// (honest, no smoothing tricks); `None` until enough data exists.
#[derive(Debug, Clone, Default, Serialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct Progress {
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub files_total: u64,
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub files_done: u64,
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub imported: u64,
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub skipped: u64,
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub failed: u64,
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub bytes_total: u64,
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub bytes_done: u64,
    pub current_file: Option<PathBuf>,
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub elapsed_ms: u64,
    #[cfg_attr(feature = "typescript", ts(type = "number | null"))]
    pub bytes_per_second: Option<u64>,
    #[cfg_attr(feature = "typescript", ts(type = "number | null"))]
    pub eta_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct FailedFile {
    pub source: PathBuf,
    pub code: ErrorCode,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct ImportResult {
    pub session_id: String,
    pub end: RunEnd,
    pub progress: Progress,
    pub failures: Vec<FailedFile>,
    /// Set if the import ran but its history record could not be written.
    pub history_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct SessionSnapshot {
    /// Strictly increasing per session object. Events and command replies
    /// travel on different channels; the GUI keeps only the highest `seq`.
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub seq: u64,
    pub state: ImportState,
    pub progress: Option<Progress>,
    pub result: Option<ImportResult>,
    /// User-facing reason for `Failed`.
    pub error: Option<SessionError>,
}

/// Error returned to the GUI: stable code + diagnostic text + path issues.
#[derive(Debug, Clone, Serialize, thiserror::Error)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
#[error("{code:?}: {message}")]
pub struct SessionError {
    pub code: ErrorCode,
    pub message: String,
    pub issues: Vec<PathIssue>,
}

impl SessionError {
    fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        SessionError {
            code,
            message: message.into(),
            issues: Vec::new(),
        }
    }
}

impl From<CoreError> for SessionError {
    fn from(e: CoreError) -> Self {
        SessionError::new(e.code(), e.to_string())
    }
}

struct Prepared {
    settings: Settings,
    source: PathBuf,
    destination: PathBuf,
    files: Vec<ScannedFile>,
}

struct Tracker {
    progress: Progress,
    started: Instant,
    in_flight: usize,
}

impl Tracker {
    fn refresh_rates(&mut self) {
        let elapsed = self.started.elapsed();
        self.progress.elapsed_ms = elapsed.as_millis() as u64;
        let secs = elapsed.as_secs_f64();
        if secs >= 0.5 && self.progress.bytes_done > 0 {
            let bps = self.progress.bytes_done as f64 / secs;
            self.progress.bytes_per_second = Some(bps as u64);
            let remaining = self
                .progress
                .bytes_total
                .saturating_sub(self.progress.bytes_done);
            self.progress.eta_ms = Some(((remaining as f64 / bps) * 1000.0) as u64);
        }
    }
}

struct Inner {
    state: ImportState,
    prepared: Option<Prepared>,
    tracker: Option<Tracker>,
    result: Option<ImportResult>,
    error: Option<SessionError>,
    last_notify: Option<Instant>,
}

struct Shared {
    paths: SessionPaths,
    seq: std::sync::atomic::AtomicU64,
    inner: Mutex<Inner>,
    control: Mutex<Arc<Control>>,
}

/// Cheap to clone; all clones refer to the same session.
#[derive(Clone)]
pub struct ImportSession {
    shared: Arc<Shared>,
}

type Notify = Arc<dyn Fn(SessionSnapshot) + Send + Sync>;

impl ImportSession {
    pub fn new(paths: SessionPaths) -> Self {
        ImportSession {
            shared: Arc::new(Shared {
                paths,
                seq: std::sync::atomic::AtomicU64::new(0),
                inner: Mutex::new(Inner {
                    state: ImportState::Idle,
                    prepared: None,
                    tracker: None,
                    result: None,
                    error: None,
                    last_notify: None,
                }),
                control: Mutex::new(Control::new()),
            }),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.shared.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn control(&self) -> Arc<Control> {
        self.shared
            .control
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn step(inner: &mut Inner, cmd: Command) -> Result<(), SessionError> {
        inner.state = transition(inner.state, cmd)?;
        Ok(())
    }

    pub fn snapshot(&self) -> SessionSnapshot {
        snapshot_of(&self.shared, &self.lock())
    }

    /// Validates the paths, scans the source and plans every destination.
    /// Blocking; call from a worker thread. Cancellable via [`Self::cancel`].
    pub fn scan(
        &self,
        source: &Path,
        destination: &Path,
        settings: Settings,
    ) -> Result<ScanPreview, SessionError> {
        // Validation happens before the state changes: invalid input is not
        // a failed scan.
        let validated = paths::validate(source, destination).map_err(|issues| SessionError {
            code: issues.first().map(|i| i.code).unwrap_or(ErrorCode::Io),
            message: "path validation failed".into(),
            issues,
        })?;
        if settings.formats.is_empty() {
            return Err(SessionError::new(
                ErrorCode::InvalidExtension,
                "no file format selected",
            ));
        }

        let control = Control::new();
        {
            let mut inner = self.lock();
            Self::step(&mut inner, Command::StartScan)?;
            *self
                .shared
                .control
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = control.clone();
            inner.prepared = None;
            inner.tracker = None;
            inner.result = None;
            inner.error = None;
        }

        let scan = scanner::scan(
            &validated.source,
            &settings.formats.matcher(),
            ScanOptions::default(),
            &control,
            |_| {},
        );
        let scan = match scan {
            Ok(s) => s,
            Err(CoreError::Cancelled) => {
                let mut inner = self.lock();
                // cancel() already moved Scanning -> Cancelling.
                let _ = Self::step(&mut inner, Command::CancelReached);
                return Err(SessionError::new(ErrorCode::Cancelled, "scan cancelled"));
            }
            Err(e) => {
                let err = SessionError::from(e);
                let mut inner = self.lock();
                let _ = Self::step(&mut inner, Command::Fail);
                inner.error = Some(err.clone());
                return Err(err);
            }
        };

        let mut missing_date_count = 0;
        let files: Vec<PreviewFile> = scan
            .files
            .iter()
            .map(|file| {
                let date = resolve_date(
                    &file.path,
                    FileTimes {
                        modified: file.modified,
                        created: file.created,
                    },
                    &settings.date_policy,
                );
                if date.is_none() {
                    missing_date_count += 1;
                }
                let relative_destination = planned_dir(
                    file,
                    Path::new(""),
                    &settings.structure,
                    &settings.date_policy,
                )
                .ok();
                PreviewFile {
                    source: file.path.clone(),
                    relative_destination,
                    size: file.size,
                    date_fallback: date.as_ref().map(|d| d.fell_back).unwrap_or(true),
                    date_source: date.map(|d| d.source),
                }
            })
            .collect();

        let preview = ScanPreview {
            files,
            total_bytes: scan.total_bytes,
            filtered_out: scan.filtered_out,
            hidden_skipped: scan.hidden_skipped,
            symlinks_skipped: scan.symlinks_skipped,
            unreadable: scan
                .unreadable
                .iter()
                .map(|s| PathIssue {
                    code: s.code,
                    path: s.path.clone(),
                })
                .collect(),
            missing_date_count,
            destination_will_be_created: validated.destination_will_be_created,
        };

        let mut inner = self.lock();
        if inner.state == ImportState::Cancelling {
            let _ = Self::step(&mut inner, Command::CancelReached);
            return Err(SessionError::new(ErrorCode::Cancelled, "scan cancelled"));
        }
        Self::step(&mut inner, Command::ScanFinished)?;
        Self::step(&mut inner, Command::BuildPreview)?;
        inner.prepared = Some(Prepared {
            settings,
            source: validated.source,
            destination: validated.destination,
            files: scan.files,
        });
        Ok(preview)
    }

    /// Starts the import of the current preview on a background thread.
    /// Only possible in `PreviewReady` (no duplicate execution).
    pub fn start_import(
        &self,
        notify: impl Fn(SessionSnapshot) + Send + Sync + 'static,
    ) -> Result<String, SessionError> {
        let notify: Notify = Arc::new(notify);
        let control = Control::new();
        let (prepared, session_id) = {
            let mut inner = self.lock();
            Self::step(&mut inner, Command::StartImport)?;
            let prepared = inner
                .prepared
                .take()
                .expect("PreviewReady always holds a prepared scan");
            *self
                .shared
                .control
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = control.clone();
            let total: u64 = prepared.files.iter().map(|f| f.size).sum();
            inner.tracker = Some(Tracker {
                progress: Progress {
                    files_total: prepared.files.len() as u64,
                    bytes_total: total,
                    ..Default::default()
                },
                started: Instant::now(),
                in_flight: 0,
            });
            inner.result = None;
            inner.error = None;
            (prepared, new_session_id())
        };

        let session = self.clone();
        let id = session_id.clone();
        std::thread::Builder::new()
            .name("raw-importer-import".into())
            .spawn(move || session.run(prepared, id, control, notify))
            .map_err(|e| {
                SessionError::new(ErrorCode::Io, format!("could not start worker: {e}"))
            })?;
        Ok(session_id)
    }

    fn run(&self, prepared: Prepared, session_id: String, control: Arc<Control>, notify: Notify) {
        let started_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let session_tag: String = session_id.chars().rev().take(8).collect();

        let opened = (|| -> Result<(HistoryStore, Journal), SessionError> {
            std::fs::create_dir_all(&prepared.destination)
                .map_err(|e| SessionError::from(CoreError::io(&prepared.destination, e)))?;
            for p in [&self.shared.paths.history_db, &self.shared.paths.journal] {
                if let Some(dir) = p.parent() {
                    std::fs::create_dir_all(dir)
                        .map_err(|e| SessionError::from(CoreError::io(dir, e)))?;
                }
            }
            let history = HistoryStore::open(&self.shared.paths.history_db)
                .map_err(|e| SessionError::new(ErrorCode::Io, format!("history database: {e}")))?;
            let journal = Journal::open(&self.shared.paths.journal)
                .map_err(|e| SessionError::new(ErrorCode::Io, format!("recovery journal: {e}")))?;
            journal
                .append(&JournalEvent::SessionStarted {
                    session_id: session_id.clone(),
                    session_tag: session_tag.clone(),
                    destination_root: prepared.destination.clone(),
                    started_at: started_at.clone(),
                })
                .map_err(|e| SessionError::new(ErrorCode::Io, format!("recovery journal: {e}")))?;
            Ok((history, journal))
        })();

        let (mut history, journal) = match opened {
            Ok(v) => v,
            Err(err) => {
                let snap = {
                    let mut inner = self.lock();
                    let _ = Self::step(&mut inner, Command::Fail);
                    inner.error = Some(err);
                    snapshot_of(&self.shared, &inner)
                };
                notify(snap);
                return;
            }
        };

        {
            let mut inner = self.lock();
            if inner.state == ImportState::Cancelling {
                // Cancelled while preparing: nothing was copied.
                let _ = Self::step(&mut inner, Command::CancelReached);
                drop(inner);
                let _ = journal.append(&JournalEvent::SessionFinished {
                    session_id: session_id.clone(),
                    status: "cancelled".into(),
                    ended_at: chrono::Utc::now().to_rfc3339(),
                });
                notify(self.snapshot());
                return;
            }
            let _ = Self::step(&mut inner, Command::ImportPrepared);
        }
        notify(self.snapshot());

        let job = ImportJob {
            destination_root: prepared.destination.clone(),
            files: prepared.files,
            structure: prepared.settings.structure,
            date_policy: prepared.settings.date_policy.clone(),
            duplicate_settings: prepared.settings.duplicates.clone(),
            conflict: prepared.settings.conflict,
            copy: CopyOptions {
                session_tag: session_tag.clone(),
                ..CopyOptions::default()
            },
            parallelism: prepared.settings.parallelism,
        };
        let context = ImportSessionContext {
            session_id: session_id.clone(),
            source_root: prepared.source.clone(),
            started_at,
        };

        let journal_ref = &journal;
        let sid = session_id.clone();
        let observer = |event: EngineEvent| {
            match &event {
                EngineEvent::CopyStarted {
                    source,
                    destination,
                    ..
                } => {
                    if let Err(e) = journal_ref.append(&JournalEvent::FileStarted {
                        session_id: sid.clone(),
                        source_path: source.clone(),
                        destination_path: destination.clone(),
                    }) {
                        tracing::error!(error = %e, "journal write failed");
                    }
                }
                EngineEvent::FileFinished { result, .. } => {
                    if let Err(e) = journal_ref.append(&JournalEvent::FileFinished {
                        session_id: sid.clone(),
                        source_path: result.source.clone(),
                    }) {
                        tracing::error!(error = %e, "journal write failed");
                    }
                }
                _ => {}
            }
            if let Some(snap) = self.apply_event(&event) {
                notify(snap);
            }
        };

        let outcome = run_import_with_history(&job, &mut history, &context, &control, &observer);
        let (summary, history_error) = match outcome {
            Ok(summary) => (Some(summary), None),
            Err(ImportRunError::HistoryWrite { summary, source }) => (
                Some(*summary),
                Some(format!("history could not be saved: {source}")),
            ),
            Err(other) => (None, Some(other.to_string())),
        };

        let snap = {
            let mut inner = self.lock();
            match summary {
                Some(summary) => {
                    let progress = finish_progress(inner.tracker.as_mut(), &summary);
                    let cmd = match summary.end {
                        RunEnd::Cancelled => Command::CancelReached,
                        RunEnd::CompletedWithErrors => Command::Finish { with_errors: true },
                        RunEnd::Completed if history_error.is_some() => {
                            Command::Finish { with_errors: true }
                        }
                        RunEnd::Completed => Command::Finish { with_errors: false },
                    };
                    if inner.state == ImportState::Paused {
                        // All files finished while paused (last ones were in flight).
                        let _ = Self::step(&mut inner, Command::Resume);
                    }
                    if cmd == Command::CancelReached && inner.state != ImportState::Cancelling {
                        let _ = Self::step(&mut inner, Command::Cancel);
                    }
                    if let Err(e) = Self::step(&mut inner, cmd) {
                        tracing::error!(error = %e, "unexpected end transition");
                        let _ = Self::step(&mut inner, Command::Fail);
                    }
                    inner.result = Some(ImportResult {
                        session_id: session_id.clone(),
                        end: summary.end,
                        progress,
                        failures: failures_of(&summary),
                        history_error: history_error.clone(),
                    });
                }
                None => {
                    let _ = Self::step(&mut inner, Command::Fail);
                    inner.error = Some(SessionError::new(
                        ErrorCode::Io,
                        history_error.clone().unwrap_or_default(),
                    ));
                }
            }
            snapshot_of(&self.shared, &inner)
        };
        let status = match snap.state {
            ImportState::Completed => "completed",
            ImportState::CompletedWithErrors => "completed_with_errors",
            ImportState::Cancelled => "cancelled",
            _ => "failed",
        };
        if let Err(e) = journal.append(&JournalEvent::SessionFinished {
            session_id,
            status: status.into(),
            ended_at: chrono::Utc::now().to_rfc3339(),
        }) {
            tracing::error!(error = %e, "journal write failed");
        }
        notify(snap);
    }

    /// Updates progress; returns a snapshot if the GUI should be notified now.
    fn apply_event(&self, event: &EngineEvent) -> Option<SessionSnapshot> {
        let mut inner = self.lock();
        let mut force = false;
        if matches!(event, EngineEvent::FileStarted { .. }) && inner.state == ImportState::Paused {
            // A worker had already passed the pause check when the pause was
            // requested: the run is not at a safe boundary yet.
            let _ = Self::step(&mut inner, Command::Pause);
            force = true;
        }
        let pausing = inner.state == ImportState::Pausing;
        let tracker = inner.tracker.as_mut()?;
        match event {
            EngineEvent::FileStarted { source, .. } => {
                tracker.in_flight += 1;
                tracker.progress.current_file = Some(source.clone());
            }
            EngineEvent::CopyStarted { .. } => {}
            EngineEvent::Bytes { delta } => tracker.progress.bytes_done += delta,
            EngineEvent::FileFinished { result, .. } => {
                tracker.in_flight = tracker.in_flight.saturating_sub(1);
                let p = &mut tracker.progress;
                match &result.outcome {
                    Outcome::Imported { .. } => {
                        p.imported += 1;
                        p.files_done += 1;
                    }
                    Outcome::Skipped { .. } => {
                        p.skipped += 1;
                        p.files_done += 1;
                        // Skipped bytes count as handled for ETA purposes.
                        p.bytes_done += result.file_size;
                    }
                    Outcome::Failed { .. } => {
                        p.failed += 1;
                        p.files_done += 1;
                    }
                    Outcome::NotProcessed => {}
                }
                force = true;
            }
        }
        tracker.refresh_rates();
        let reached_pause = pausing && tracker.in_flight == 0;
        if reached_pause {
            let _ = Self::step(&mut inner, Command::PauseReached);
            force = true;
        }
        let now = Instant::now();
        let due = inner
            .last_notify
            .map(|t| now.duration_since(t) >= NOTIFY_INTERVAL)
            .unwrap_or(true);
        if force || due {
            inner.last_notify = Some(now);
            Some(snapshot_of(&self.shared, &inner))
        } else {
            None
        }
    }

    /// Stops starting new files; files in progress finish (safe boundary).
    /// The state is `Pausing` until no file is in flight, then `Paused`.
    pub fn pause(&self) -> Result<SessionSnapshot, SessionError> {
        {
            let mut inner = self.lock();
            Self::step(&mut inner, Command::Pause)?;
        }
        // Lock order is always Control before Inner (workers report events
        // while holding neither), so the flag is set without holding Inner.
        self.control().set_paused(true);
        let mut inner = self.lock();
        let idle = inner
            .tracker
            .as_ref()
            .map(|t| t.in_flight == 0)
            .unwrap_or(true);
        if inner.state == ImportState::Pausing && idle {
            Self::step(&mut inner, Command::PauseReached)?;
        }
        Ok(snapshot_of(&self.shared, &inner))
    }

    pub fn resume(&self) -> Result<SessionSnapshot, SessionError> {
        {
            let mut inner = self.lock();
            Self::step(&mut inner, Command::Resume)?;
        }
        self.control().set_paused(false);
        Ok(self.snapshot())
    }

    /// Requests cancellation of a running scan or import. The copy in
    /// progress stops at the next chunk; its temp file is removed.
    pub fn cancel(&self) -> Result<SessionSnapshot, SessionError> {
        {
            let mut inner = self.lock();
            Self::step(&mut inner, Command::Cancel)?;
        }
        self.control().cancel();
        Ok(self.snapshot())
    }

    /// Returns to `Idle` after a finished, cancelled or failed run.
    pub fn reset(&self) -> Result<SessionSnapshot, SessionError> {
        let mut inner = self.lock();
        Self::step(&mut inner, Command::Reset)?;
        inner.prepared = None;
        inner.tracker = None;
        inner.result = None;
        inner.error = None;
        Ok(snapshot_of(&self.shared, &inner))
    }
}

fn snapshot_of(shared: &Shared, inner: &Inner) -> SessionSnapshot {
    SessionSnapshot {
        // Taken while `inner` is locked, so seq order equals state order.
        seq: shared.seq.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1,
        state: inner.state,
        progress: inner.tracker.as_ref().map(|t| t.progress.clone()),
        result: inner.result.clone(),
        error: inner.error.clone(),
    }
}

fn finish_progress(tracker: Option<&mut Tracker>, summary: &RunSummary) -> Progress {
    let c = &summary.counters;
    let mut p = tracker
        .map(|t| {
            t.refresh_rates();
            t.progress.clone()
        })
        .unwrap_or_default();
    // The summary is authoritative for the final numbers.
    p.files_total = c.total as u64;
    p.imported = c.imported as u64;
    p.skipped = c.skipped as u64;
    p.failed = c.failed as u64;
    p.files_done = (c.imported + c.skipped + c.failed) as u64;
    p.bytes_total = c.bytes_total;
    p.current_file = None;
    p.eta_ms = None;
    p
}

fn failures_of(summary: &RunSummary) -> Vec<FailedFile> {
    summary
        .results
        .iter()
        .filter_map(|r| match &r.outcome {
            Outcome::Failed { code, detail } => Some(FailedFile {
                source: r.source.clone(),
                code: *code,
                detail: detail.clone(),
            }),
            _ => None,
        })
        .collect()
}

fn new_session_id() -> String {
    let now = chrono::Utc::now();
    let nanos = now.timestamp_nanos_opt().unwrap_or_default() as u128;
    format!(
        "{}-{:08x}",
        now.format("%Y%m%dT%H%M%S"),
        (nanos ^ ((std::process::id() as u128) << 32)) as u32
    )
}
