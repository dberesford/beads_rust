//! `beads_rust` - Agent-first issue tracker library
//!
//! This crate provides the core functionality for the `br` CLI tool,
//! a Rust port of the classic beads issue tracker.
//!
//! # Architecture
//!
//! The crate is organized into the following modules:
//!
//! - [`cli`] - Command-line interface using clap
//! - [`model`] - Data types (Issue, Dependency, Comment, Event)
//! - [`storage`] - `SQLite` database layer
//! - [`sync`] - JSONL import/export operations
//! - [`config`] - Configuration management
//! - [`error`] - Error types and handling
//! - [`format`] - Output formatting (text, JSON)
//! - [`util`] - Utility functions (hashing, time, paths)

#![forbid(unsafe_code)]
#![warn(clippy::pedantic, clippy::nursery)]
#![allow(clippy::module_name_repetitions)]

pub mod cli;
pub mod config;
pub mod error;
pub mod format;
pub mod logging;
pub mod model;
pub mod storage;
pub mod sync;
pub mod util;
pub mod validation;

pub use error::{BeadsError, Result};

/// Run the CLI application.
///
/// This is the main entry point called from `main()`.
///
/// # Errors
///
/// Returns an error if command execution fails.
#[allow(clippy::missing_const_for_fn)] // Will have side effects once implemented
pub fn run() -> Result<()> {
    // Initialize logging
    // Parse CLI arguments
    // Execute command
    // TODO: Implement CLI dispatch
    Ok(())
}
