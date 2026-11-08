//! RAW Importer 3.0 — import engine.
//!
//! This crate contains everything that decides *what* is copied *where* and
//! performs the copy safely. It has no GUI dependency so it can be tested with
//! `cargo test` alone (see ARCHITECTURE.md, decision D-001).

pub mod copier;
pub mod destination;
pub mod duplicates;
pub mod engine;
pub mod error;
pub mod formats;
pub mod history;
pub mod journal;
pub mod metadata;
pub mod paths;
pub mod platform;
pub mod scanner;
pub mod session;
pub mod settings;
pub mod state;
pub mod updates;

pub use error::{CoreError, ErrorCode};
