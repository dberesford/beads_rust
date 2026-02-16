//! Error types for `beads-lib`.
//!
//! Simplified `BeadsError` without SQLite-specific variants.

use std::path::PathBuf;
use thiserror::Error;

/// Primary error type for beads-lib operations.
#[derive(Error, Debug)]
pub enum BeadsError {
    // === Issue Errors ===
    /// Issue with the specified ID was not found.
    #[error("Issue not found: {id}")]
    IssueNotFound { id: String },

    /// Attempted to create an issue with an ID that already exists.
    #[error("Issue ID collision: {id}")]
    IdCollision { id: String },

    /// Partial ID matches multiple issues.
    #[error("Ambiguous ID '{partial}': matches {matches:?}")]
    AmbiguousId {
        partial: String,
        matches: Vec<String>,
    },

    /// Issue ID format is invalid.
    #[error("Invalid issue ID format: {id}")]
    InvalidId { id: String },

    // === Validation Errors ===
    /// Field validation failed.
    #[error("Validation failed: {field}: {reason}")]
    Validation { field: String, reason: String },

    /// Multiple validation errors occurred.
    #[error("Validation errors: {errors:?}")]
    ValidationErrors { errors: Vec<ValidationError> },

    /// Invalid status value.
    #[error("Invalid status: {status}")]
    InvalidStatus { status: String },

    /// Invalid issue type value.
    #[error("Invalid issue type: {issue_type}")]
    InvalidType { issue_type: String },

    /// Priority out of valid range (0-4).
    #[error("Priority must be 0-4, got: {priority}")]
    InvalidPriority { priority: i32 },

    // === JSONL Errors ===
    /// Failed to parse a line in the JSONL file.
    #[error("JSONL parse error at line {line}: {reason}")]
    JsonlParse { line: usize, reason: String },

    /// Issue prefix doesn't match expected prefix.
    #[error("Prefix mismatch: expected '{expected}', found '{found}'")]
    PrefixMismatch { expected: String, found: String },

    // === Dependency Errors ===
    /// Adding the dependency would create a cycle.
    #[error("Cycle detected in dependencies: {path}")]
    DependencyCycle { path: String },

    /// Cannot delete an issue that has dependents.
    #[error("Cannot delete: {id} has {count} dependents")]
    HasDependents { id: String, count: usize },

    /// Self-referential dependency.
    #[error("Issue cannot depend on itself: {id}")]
    SelfDependency { id: String },

    /// Dependency target not found.
    #[error("Dependency target not found: {id}")]
    DependencyNotFound { id: String },

    /// Duplicate dependency.
    #[error("Dependency already exists: {from} -> {to}")]
    DuplicateDependency { from: String, to: String },

    // === Configuration Errors ===
    /// Configuration error.
    #[error("Configuration error: {0}")]
    Config(String),

    // === Storage Errors ===
    /// Generic storage error (replaces SQLite-specific variants).
    #[error("Storage error: {0}")]
    Storage(String),

    /// File not found at the specified path.
    #[error("File not found: {0}")]
    FileNotFound(PathBuf),

    // === I/O Errors ===
    /// File system I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization/deserialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    // === Operational Errors ===
    /// All requested items were skipped.
    #[error("Nothing to do: {reason}")]
    NothingToDo { reason: String },
}

/// A single field validation error.
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
}

impl ValidationError {
    #[must_use]
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.field, self.message)
    }
}

impl std::error::Error for ValidationError {}

impl BeadsError {
    #[must_use]
    pub fn validation(field: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Validation {
            field: field.into(),
            reason: reason.into(),
        }
    }

    #[must_use]
    pub fn from_validation_errors(errors: Vec<ValidationError>) -> Self {
        if errors.len() == 1 {
            let err = &errors[0];
            Self::Validation {
                field: err.field.clone(),
                reason: err.message.clone(),
            }
        } else {
            Self::ValidationErrors { errors }
        }
    }
}

/// Result type using `BeadsError`.
pub type Result<T> = std::result::Result<T, BeadsError>;
