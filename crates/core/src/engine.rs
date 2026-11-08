//! Import execution: plans each file, resolves conflicts, copies safely and
//! reports per-file results. Runs with bounded parallelism (master prompt §13).
//!
//! The engine does not decide *whether* an import may start; the caller drives
//! the [`crate::state`] machine and calls [`run_import`] in state `Importing`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use serde::Serialize;

use crate::copier::{copy_file, CopyOptions};
use crate::destination::{plan_relative_dir, StructureOptions};
use crate::duplicates::{
    decide, detect_content_duplicates, ConflictPolicy, ContentDuplicateError,
    ContentDuplicateReport, Decision, DuplicateScope, DuplicateSearchContext, Incoming,
    Reservations, SkipReason,
};
use crate::error::{CoreError, ErrorCode};
use crate::history::{HistoryError, HistoryStore};
use crate::metadata::{resolve_date, DatePolicy, FileTimes, ResolvedDate};
use crate::scanner::ScannedFile;
use crate::settings::DuplicateSettings;
use crate::state::Control;

/// Worker count. `Auto` is deliberately small: imports are I/O-bound and a
/// single card reader or HDD gets slower with many parallel streams (2.x used
/// the CPU core count, B12). Tuned by measurements in phase 8.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "mode", content = "workers")]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum Parallelism {
    Sequential,
    #[default]
    Auto,
    Manual(u8),
}

pub const MAX_WORKERS: usize = 8;

type WorkerSlot = Mutex<Option<(Outcome, Option<ContentDuplicateReport>)>>;

impl Parallelism {
    pub fn workers(self) -> usize {
        match self {
            Parallelism::Sequential => 1,
            Parallelism::Auto => 2,
            Parallelism::Manual(n) => (n as usize).clamp(1, MAX_WORKERS),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImportJob {
    pub destination_root: PathBuf,
    pub files: Vec<ScannedFile>,
    pub structure: StructureOptions,
    pub date_policy: DatePolicy,
    pub duplicate_settings: DuplicateSettings,
    pub conflict: ConflictPolicy,
    pub copy: CopyOptions,
    pub parallelism: Parallelism,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Outcome {
    Imported {
        destination: PathBuf,
        bytes: u64,
    },
    Skipped {
        reason: SkipReason,
        existing: PathBuf,
    },
    Failed {
        code: ErrorCode,
        detail: String,
    },
    /// Not processed because the run was cancelled first.
    NotProcessed,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileResult {
    pub source: PathBuf,
    pub file_size: u64,
    pub outcome: Outcome,
    pub duplicate_check: Option<ContentDuplicateReport>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum RunEnd {
    Completed,
    CompletedWithErrors,
    Cancelled,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Counters {
    pub total: usize,
    pub imported: usize,
    pub skipped: usize,
    pub failed: usize,
    pub bytes_total: u64,
    pub bytes_done: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunSummary {
    pub end: RunEnd,
    pub counters: Counters,
    pub results: Vec<FileResult>,
}

#[derive(Debug, Clone)]
pub struct ImportSessionContext {
    pub session_id: String,
    pub source_root: PathBuf,
    pub started_at: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ImportRunError {
    #[error(transparent)]
    History(#[from] HistoryError),
    #[error(transparent)]
    Duplicate(#[from] ContentDuplicateError),
    #[error("import finished, but its results could not be saved to history: {source}")]
    HistoryWrite {
        summary: Box<RunSummary>,
        #[source]
        source: HistoryError,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum EngineEvent {
    FileStarted {
        index: usize,
        source: PathBuf,
    },
    /// The destination name is reserved and the copy is about to begin.
    /// Emitted before any byte is written; used by the recovery journal.
    CopyStarted {
        index: usize,
        source: PathBuf,
        destination: PathBuf,
    },
    Bytes {
        delta: u64,
    },
    FileFinished {
        index: usize,
        result: FileResult,
    },
}

/// Resolves the folder date using the job policy and the scanner's file times.
pub fn file_date(file: &ScannedFile, policy: &DatePolicy) -> Option<ResolvedDate> {
    resolve_date(
        &file.path,
        FileTimes {
            modified: file.modified,
            created: file.created,
        },
        policy,
    )
}

struct Worker<'a> {
    job: &'a ImportJob,
    reservations: &'a Reservations,
    control: &'a Control,
    observer: &'a (dyn Fn(EngineEvent) + Sync),
}

fn process_one(index: usize, file: &ScannedFile, w: &Worker<'_>) -> Outcome {
    let Worker {
        job,
        reservations,
        control,
        observer,
    } = *w;
    let on_bytes = |delta: u64| observer(EngineEvent::Bytes { delta });
    let Some(date) = file_date(file, &job.date_policy) else {
        return failed(&CoreError::DateUnavailable(file.path.clone()));
    };
    let rel = match plan_relative_dir(date.date, &file.rel_dir, file.file_name(), &job.structure) {
        Ok(r) => r,
        Err(e) => return failed(&e),
    };
    let dir = job.destination_root.join(rel);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return failed(&CoreError::io(&dir, e));
    }

    // A name can be taken by another process between decision and finalise;
    // then we decide again (bounded).
    for _ in 0..3 {
        let decision = match decide(
            Incoming {
                path: &file.path,
                size: file.size,
                file_name: file.file_name(),
            },
            &dir,
            job.conflict,
            reservations,
            &job.copy.session_tag,
            control,
        ) {
            Ok(d) => d,
            Err(e) => return failed(&e),
        };
        let target = match decision {
            Decision::Skip { reason, existing } => return Outcome::Skipped { reason, existing },
            Decision::Copy(p) => p,
        };
        observer(EngineEvent::CopyStarted {
            index,
            source: file.path.clone(),
            destination: target.clone(),
        });
        let result = copy_file(&file.path, &target, &job.copy, control, on_bytes);
        reservations.release(&target);
        match result {
            Ok(report) => {
                return Outcome::Imported {
                    destination: target,
                    bytes: report.bytes,
                }
            }
            Err(CoreError::DestinationExists(_)) => continue,
            Err(e) => return failed(&e),
        }
    }
    failed(&CoreError::DestinationExists(dir.join(file.file_name())))
}

fn failed(e: &CoreError) -> Outcome {
    Outcome::Failed {
        code: e.code(),
        detail: e.to_string(),
    }
}

/// Runs the import. Blocks until all workers finished; call it from a
/// background thread. `observer` must be cheap (throttle in the caller).
pub fn run_import(
    job: &ImportJob,
    control: &Control,
    observer: &(dyn Fn(EngineEvent) + Sync),
) -> RunSummary {
    if job.duplicate_settings.content_detection_enabled {
        return history_required_summary(job);
    }
    run_import_inner(
        job,
        control,
        observer,
        vec![None; job.files.len()],
        vec![None; job.files.len()],
    )
}

/// Runs a job after hash-based duplicate preflight using the supplied SQLite store.
/// Preflight is deterministic and completes before copy workers start.
pub fn run_import_with_history(
    job: &ImportJob,
    history: &mut HistoryStore,
    session: &ImportSessionContext,
    control: &Control,
    observer: &(dyn Fn(EngineEvent) + Sync),
) -> Result<RunSummary, ImportRunError> {
    let mut preflight_outcomes = vec![None; job.files.len()];
    let mut duplicate_checks = vec![None; job.files.len()];
    if job.duplicate_settings.content_detection_enabled {
        let previous_imports = if job
            .duplicate_settings
            .scopes
            .contains(&DuplicateScope::PreviousImports)
        {
            history.previous_import_paths()?
        } else {
            Vec::new()
        };

        for (index, file) in job.files.iter().enumerate() {
            if file_date(file, &job.date_policy).is_none() {
                continue;
            }
            let planned = match planned_dir(
                file,
                &job.destination_root,
                &job.structure,
                &job.date_policy,
            ) {
                Ok(path) => path,
                Err(_) => continue,
            };
            let current_session_files: Vec<_> = job.files[..index]
                .iter()
                .map(|scanned| scanned.path.clone())
                .collect();
            let report = detect_content_duplicates(
                history,
                &job.duplicate_settings,
                &file.path,
                &DuplicateSearchContext {
                    planned_destination_dir: &planned,
                    destination_root: &job.destination_root,
                    current_session_files: &current_session_files,
                    previous_imports: &previous_imports,
                },
                control,
            )?;
            if let Some(report) = report {
                if job.conflict == ConflictPolicy::SkipIdentical {
                    if let Some(existing) = report.matches.first() {
                        preflight_outcomes[index] = Some(Outcome::Skipped {
                            reason: SkipReason::IdenticalContent,
                            existing: existing.path.clone(),
                        });
                    }
                }
                duplicate_checks[index] = Some(report);
            }
        }
    }
    let summary = run_import_inner(job, control, observer, preflight_outcomes, duplicate_checks);
    let history_result = history_session(job, session, &summary).and_then(|history_session| {
        let files = history_files(job, &summary);
        history.record_session(&history_session, &files)
    });
    if let Err(source) = history_result {
        return Err(ImportRunError::HistoryWrite {
            summary: Box::new(summary.clone()),
            source,
        });
    }
    Ok(summary)
}

fn history_session(
    job: &ImportJob,
    context: &ImportSessionContext,
    summary: &RunSummary,
) -> Result<crate::history::HistorySession, HistoryError> {
    let settings_json = serde_json::to_string(&serde_json::json!({
        "structure": job.structure,
        "date_policy": job.date_policy,
        "duplicate_settings": job.duplicate_settings,
        "conflict": job.conflict,
        "parallelism": job.parallelism,
        "verify_hash": job.copy.verify_hash,
        "preserve_mtime": job.copy.preserve_mtime,
    }))?;
    let status = match summary.end {
        RunEnd::Completed => "completed",
        RunEnd::CompletedWithErrors => "completed_with_errors",
        RunEnd::Cancelled => "cancelled",
    };
    Ok(crate::history::HistorySession {
        session_id: context.session_id.clone(),
        started_at: context.started_at.clone(),
        ended_at: Some(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Nanos, true)),
        source_path: context.source_root.clone(),
        destination_path: job.destination_root.clone(),
        status: status.into(),
        total_files: summary.counters.total as u64,
        imported_files: summary.counters.imported as u64,
        skipped_files: summary.counters.skipped as u64,
        failed_files: summary.counters.failed as u64,
        bytes_total: summary.counters.bytes_total,
        bytes_done: summary.counters.bytes_done,
        settings_json,
    })
}

fn history_files(job: &ImportJob, summary: &RunSummary) -> Vec<crate::history::HistoryFile> {
    let scanned_by_path: HashMap<_, _> = job
        .files
        .iter()
        .map(|file| (file.path.as_path(), file))
        .collect();
    summary
        .results
        .iter()
        .map(|result| {
            let planned = scanned_by_path
                .get(result.source.as_path())
                .and_then(|file| {
                    planned_dir(
                        file,
                        &job.destination_root,
                        &job.structure,
                        &job.date_policy,
                    )
                    .ok()
                    .map(|dir| dir.join(file.file_name()))
                });
            let (outcome, destination_path, error_code, detail) = match &result.outcome {
                Outcome::Imported {
                    destination,
                    bytes: _,
                } => ("imported", Some(destination.clone()), None, None),
                Outcome::Skipped { reason, .. } => {
                    let name = match reason {
                        SkipReason::DestinationExists => "skipped_destination_exists",
                        SkipReason::IdenticalContent => "skipped_identical_content",
                    };
                    (name, planned, None, None)
                }
                Outcome::Failed { code, detail } => {
                    let error_code = serde_json::to_value(code)
                        .ok()
                        .and_then(|value| value.as_str().map(str::to_owned));
                    ("failed", planned, error_code, Some(detail.clone()))
                }
                Outcome::NotProcessed => ("not_processed", planned, None, None),
            };
            let duplicate_status = result.duplicate_check.as_ref().map(|report| {
                if report.matches.is_empty() {
                    if report.candidate_paths_checked == 0 {
                        "no_candidates".to_string()
                    } else {
                        "verified_unique".to_string()
                    }
                } else {
                    "verified_duplicate".to_string()
                }
            });
            crate::history::HistoryFile {
                source_path: result.source.clone(),
                destination_path,
                file_size: Some(result.file_size),
                outcome: outcome.into(),
                error_code,
                detail,
                detection_method: result
                    .duplicate_check
                    .as_ref()
                    .map(|_| "sha256".to_string()),
                hash_cache_used: result
                    .duplicate_check
                    .as_ref()
                    .is_some_and(|report| report.cached_hashes_used > 0),
                sha256: result
                    .duplicate_check
                    .as_ref()
                    .map(|report| report.source_hash.sha256),
                hash_algorithm_version: result.duplicate_check.as_ref().map(|_| 1),
                duplicate_status,
            }
        })
        .collect()
}

fn history_required_summary(job: &ImportJob) -> RunSummary {
    let detail = "content duplicate detection requires run_import_with_history".to_string();
    let results: Vec<_> = job
        .files
        .iter()
        .map(|file| FileResult {
            source: file.path.clone(),
            file_size: file.size,
            outcome: Outcome::Failed {
                code: ErrorCode::Database,
                detail: detail.clone(),
            },
            duplicate_check: None,
        })
        .collect();
    let failed = results.len();
    RunSummary {
        end: if failed == 0 {
            RunEnd::Completed
        } else {
            RunEnd::CompletedWithErrors
        },
        counters: Counters {
            total: failed,
            failed,
            bytes_total: job.files.iter().map(|file| file.size).sum(),
            ..Default::default()
        },
        results,
    }
}

fn run_import_inner(
    job: &ImportJob,
    control: &Control,
    observer: &(dyn Fn(EngineEvent) + Sync),
    preflight_outcomes: Vec<Option<Outcome>>,
    duplicate_checks: Vec<Option<ContentDuplicateReport>>,
) -> RunSummary {
    let n = job.files.len();
    let next = AtomicUsize::new(0);
    let reservations = Reservations::default();
    let slots: Vec<WorkerSlot> = (0..n).map(|_| Mutex::new(None)).collect();
    let worker = Worker {
        job,
        reservations: &reservations,
        control,
        observer,
    };

    let workers = job.parallelism.workers().min(n.max(1));
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| loop {
                // Safe boundary: pause/cancel take effect between files.
                if !control.wait_if_paused() {
                    break;
                }
                let i = next.fetch_add(1, Ordering::SeqCst);
                if i >= n {
                    break;
                }
                let file = &job.files[i];
                observer(EngineEvent::FileStarted {
                    index: i,
                    source: file.path.clone(),
                });
                let duplicate_check = duplicate_checks[i].clone();
                let mut outcome = preflight_outcomes[i]
                    .clone()
                    .unwrap_or_else(|| process_one(i, file, &worker));
                if matches!(
                    &outcome,
                    Outcome::Failed {
                        code: ErrorCode::Cancelled,
                        ..
                    }
                ) {
                    outcome = Outcome::NotProcessed;
                }
                observer(EngineEvent::FileFinished {
                    index: i,
                    result: FileResult {
                        source: file.path.clone(),
                        file_size: file.size,
                        outcome: outcome.clone(),
                        duplicate_check: duplicate_check.clone(),
                    },
                });
                *slots[i].lock().unwrap_or_else(|e| e.into_inner()) =
                    Some((outcome, duplicate_check));
            });
        }
    });

    let mut counters = Counters {
        total: n,
        bytes_total: job.files.iter().map(|f| f.size).sum(),
        ..Default::default()
    };
    let results: Vec<FileResult> = job
        .files
        .iter()
        .zip(slots)
        .map(|(f, slot)| {
            let (outcome, duplicate_check) = slot
                .into_inner()
                .unwrap_or_else(|e| e.into_inner())
                .unwrap_or((Outcome::NotProcessed, None));
            match &outcome {
                Outcome::Imported { bytes, .. } => {
                    counters.imported += 1;
                    counters.bytes_done += bytes;
                }
                Outcome::Skipped { .. } => counters.skipped += 1,
                Outcome::Failed { .. } => counters.failed += 1,
                Outcome::NotProcessed => {}
            }
            FileResult {
                source: f.path.clone(),
                file_size: f.size,
                outcome,
                duplicate_check,
            }
        })
        .collect();

    let end = if control.is_cancelled() {
        RunEnd::Cancelled
    } else if counters.failed > 0 {
        RunEnd::CompletedWithErrors
    } else {
        RunEnd::Completed
    };
    RunSummary {
        end,
        counters,
        results,
    }
}

/// Convenience for previews: the planned destination directory of a file.
pub fn planned_dir(
    file: &ScannedFile,
    job_root: &Path,
    structure: &StructureOptions,
    date_policy: &DatePolicy,
) -> Result<PathBuf, CoreError> {
    let date = file_date(file, date_policy)
        .ok_or_else(|| CoreError::DateUnavailable(file.path.clone()))?;
    Ok(job_root.join(plan_relative_dir(
        date.date,
        &file.rel_dir,
        file.file_name(),
        structure,
    )?))
}
