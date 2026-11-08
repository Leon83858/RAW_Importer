//! Import state machine and run control (cancel / pause).
//!
//! The backend is authoritative for the state (master prompt §6). Every
//! transition goes through [`transition`]; anything not listed is rejected.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use serde::Serialize;

use crate::error::{CoreError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum ImportState {
    Idle,
    Scanning,
    ScanCompleted,
    PreviewReady,
    PreparingImport,
    Importing,
    Pausing,
    Paused,
    Cancelling,
    Cancelled,
    Completed,
    CompletedWithErrors,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Command {
    StartScan,
    ScanFinished,
    BuildPreview,
    StartImport,
    ImportPrepared,
    Pause,
    PauseReached,
    Resume,
    Cancel,
    CancelReached,
    Finish { with_errors: bool },
    Fail,
    Reset,
}

impl ImportState {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            ImportState::Cancelled
                | ImportState::Completed
                | ImportState::CompletedWithErrors
                | ImportState::Failed
        )
    }

    pub fn is_busy(self) -> bool {
        matches!(
            self,
            ImportState::Scanning
                | ImportState::PreparingImport
                | ImportState::Importing
                | ImportState::Pausing
                | ImportState::Paused
                | ImportState::Cancelling
        )
    }
}

/// Returns the next state or [`CoreError::InvalidTransition`].
pub fn transition(from: ImportState, cmd: Command) -> Result<ImportState> {
    use Command as C;
    use ImportState as S;
    let next = match (from, cmd) {
        // A new scan is allowed whenever nothing is running.
        (S::Idle | S::ScanCompleted | S::PreviewReady, C::StartScan) => S::Scanning,
        (s, C::StartScan) if s.is_terminal() => S::Scanning,
        (S::Scanning, C::ScanFinished) => S::ScanCompleted,
        (S::ScanCompleted, C::BuildPreview) => S::PreviewReady,
        // Only a reviewed preview can be imported -> no duplicate execution.
        (S::PreviewReady, C::StartImport) => S::PreparingImport,
        (S::PreparingImport, C::ImportPrepared) => S::Importing,
        (S::Importing, C::Pause) => S::Pausing,
        (S::Pausing, C::PauseReached) => S::Paused,
        (S::Paused | S::Pausing, C::Resume) => S::Importing,
        (S::Scanning, C::Cancel) => S::Cancelling,
        (S::PreparingImport | S::Importing | S::Pausing | S::Paused, C::Cancel) => S::Cancelling,
        (S::Cancelling, C::CancelReached) => S::Cancelled,
        (S::Importing | S::Pausing, C::Finish { with_errors: false }) => S::Completed,
        (S::Importing | S::Pausing, C::Finish { with_errors: true }) => S::CompletedWithErrors,
        (s, C::Fail) if s.is_busy() => S::Failed,
        (s, C::Reset) if !s.is_busy() => S::Idle,
        _ => {
            return Err(CoreError::InvalidTransition {
                from: format!("{from:?}"),
                command: format!("{cmd:?}"),
            })
        }
    };
    Ok(next)
}

/// Shared cancel/pause flags, checked by workers at safe boundaries.
#[derive(Debug, Default)]
pub struct Control {
    cancelled: AtomicBool,
    paused: Mutex<bool>,
    wake: Condvar,
}

impl Control {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        // Wake paused workers so they can observe the cancellation.
        let _guard = self.paused.lock().unwrap_or_else(|e| e.into_inner());
        self.wake.notify_all();
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub fn set_paused(&self, paused: bool) {
        let mut p = self.paused.lock().unwrap_or_else(|e| e.into_inner());
        *p = paused;
        self.wake.notify_all();
    }

    pub fn is_paused(&self) -> bool {
        *self.paused.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Blocks while paused. Returns `false` if the run was cancelled.
    pub fn wait_if_paused(&self) -> bool {
        let mut p = self.paused.lock().unwrap_or_else(|e| e.into_inner());
        while *p && !self.is_cancelled() {
            p = self.wake.wait(p).unwrap_or_else(|e| e.into_inner());
        }
        !self.is_cancelled()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ImportState as S;

    const ALL_STATES: [S; 13] = [
        S::Idle,
        S::Scanning,
        S::ScanCompleted,
        S::PreviewReady,
        S::PreparingImport,
        S::Importing,
        S::Pausing,
        S::Paused,
        S::Cancelling,
        S::Cancelled,
        S::Completed,
        S::CompletedWithErrors,
        S::Failed,
    ];

    fn all_commands() -> Vec<Command> {
        use Command::*;
        vec![
            StartScan,
            ScanFinished,
            BuildPreview,
            StartImport,
            ImportPrepared,
            Pause,
            PauseReached,
            Resume,
            Cancel,
            CancelReached,
            Finish { with_errors: false },
            Finish { with_errors: true },
            Fail,
            Reset,
        ]
    }

    #[test]
    fn happy_path() {
        let mut s = S::Idle;
        for c in [
            Command::StartScan,
            Command::ScanFinished,
            Command::BuildPreview,
            Command::StartImport,
            Command::ImportPrepared,
            Command::Pause,
            Command::PauseReached,
            Command::Resume,
            Command::Finish { with_errors: true },
        ] {
            s = transition(s, c).unwrap();
        }
        assert_eq!(s, S::CompletedWithErrors);
    }

    #[test]
    fn start_import_only_from_preview_ready() {
        // AC-10: duplicate execution is impossible.
        for s in ALL_STATES {
            let r = transition(s, Command::StartImport);
            assert_eq!(r.is_ok(), s == S::PreviewReady, "{s:?}");
        }
    }

    #[test]
    fn exhaustive_table_has_no_panics_and_busy_states_cannot_reset() {
        let mut allowed = 0;
        for s in ALL_STATES {
            for c in all_commands() {
                if let Ok(next) = transition(s, c) {
                    allowed += 1;
                    if c == Command::Reset {
                        assert!(!s.is_busy());
                        assert_eq!(next, S::Idle);
                    }
                }
            }
        }
        // Guard against accidentally widening the table. All 38 transitions
        // were reviewed on 2026-11-08 (see IMPLEMENTATION_PLAN.md AC-10).
        assert_eq!(allowed, 38);
    }

    #[test]
    fn terminal_states_never_go_back_to_importing() {
        for s in ALL_STATES.into_iter().filter(|s| s.is_terminal()) {
            for c in all_commands() {
                if let Ok(next) = transition(s, c) {
                    assert!(matches!(next, S::Idle | S::Scanning), "{s:?} -> {next:?}");
                }
            }
        }
    }

    #[test]
    fn cancel_wakes_paused_waiters() {
        let ctl = Control::new();
        ctl.set_paused(true);
        let c2 = ctl.clone();
        let h = std::thread::spawn(move || c2.wait_if_paused());
        std::thread::sleep(std::time::Duration::from_millis(50));
        ctl.cancel();
        assert!(!h.join().unwrap());
    }
}
