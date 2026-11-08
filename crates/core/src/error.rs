//! Structured errors.
//!
//! Every error carries a stable [`ErrorCode`] that the frontend maps to a
//! localised, actionable message. The `Display` text is the internal
//! diagnostic description and is meant for logs, not for end users.

use std::path::PathBuf;

use serde::Serialize;

/// Stable, language-independent error identifiers shown to the frontend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum ErrorCode {
    SourceMissing,
    SourceNotDirectory,
    SourceUnreadable,
    DestinationNotDirectory,
    DestinationParentMissing,
    DestinationUnwritable,
    SameSourceAndDestination,
    DestinationInsideSource,
    SourceInsideDestination,
    InvalidExtension,
    UnsafeRelativePath,
    SourceVanished,
    SourceChanged,
    DateUnavailable,
    PermissionDenied,
    DiskFull,
    SizeMismatch,
    HashMismatch,
    DestinationExists,
    Cancelled,
    InvalidTransition,
    Database,
    Io,
}

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("{code:?}: {path}")]
    Path { code: ErrorCode, path: PathBuf },

    #[error("invalid extension {0:?}")]
    InvalidExtension(String),

    #[error("relative path {0:?} would leave the destination root")]
    UnsafeRelativePath(PathBuf),

    #[error("source changed during copy: {0}")]
    SourceChanged(PathBuf),

    #[error("no usable date metadata for source: {0}")]
    DateUnavailable(PathBuf),

    #[error("copied size {written} differs from source size {expected}: {path}")]
    SizeMismatch {
        path: PathBuf,
        expected: u64,
        written: u64,
    },

    #[error("hash of copy differs from source: {0}")]
    HashMismatch(PathBuf),

    #[error("destination already exists: {0}")]
    DestinationExists(PathBuf),

    #[error("operation cancelled")]
    Cancelled,

    #[error("database operation failed: {0}")]
    Database(String),

    #[error("invalid state transition from {from} via {command}")]
    InvalidTransition { from: String, command: String },

    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

impl CoreError {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        CoreError::Io {
            path: path.into(),
            source,
        }
    }

    /// The stable code used by the frontend to pick a message.
    pub fn code(&self) -> ErrorCode {
        match self {
            CoreError::Path { code, .. } => *code,
            CoreError::InvalidExtension(_) => ErrorCode::InvalidExtension,
            CoreError::UnsafeRelativePath(_) => ErrorCode::UnsafeRelativePath,
            CoreError::SourceChanged(_) => ErrorCode::SourceChanged,
            CoreError::DateUnavailable(_) => ErrorCode::DateUnavailable,
            CoreError::SizeMismatch { .. } => ErrorCode::SizeMismatch,
            CoreError::HashMismatch(_) => ErrorCode::HashMismatch,
            CoreError::DestinationExists(_) => ErrorCode::DestinationExists,
            CoreError::Cancelled => ErrorCode::Cancelled,
            CoreError::Database(_) => ErrorCode::Database,
            CoreError::InvalidTransition { .. } => ErrorCode::InvalidTransition,
            CoreError::Io { source, .. } => io_code(source),
        }
    }
}

fn io_code(err: &std::io::Error) -> ErrorCode {
    use std::io::ErrorKind;
    match err.kind() {
        ErrorKind::NotFound => ErrorCode::SourceVanished,
        ErrorKind::PermissionDenied => ErrorCode::PermissionDenied,
        ErrorKind::AlreadyExists => ErrorCode::DestinationExists,
        ErrorKind::StorageFull => ErrorCode::DiskFull,
        _ => ErrorCode::Io,
    }
}

pub type Result<T, E = CoreError> = std::result::Result<T, E>;
