//! Output formatting for `beads_rust`.
//!
//! Supports both human-readable text output and machine-parseable JSON.
//! Robot mode sends clean JSON to stdout with diagnostics to stderr.
//!
//! # JSON Output Types
//!
//! These types match the classic bd JSON schemas for CLI compatibility:
//! - [`IssueWithCounts`] - Issue with dependency/dependent counts (list/search)
//! - [`IssueDetails`] - Issue with full relations (show)
//! - [`BlockedIssue`] - Issue with blocking info (blocked)
//! - [`TreeNode`] - Issue in dependency tree (dep tree)
//! - [`Statistics`] - Aggregate stats (stats/status)

mod output;
mod text;

pub use output::{
    BlockedIssue, IssueDetails, IssueWithCounts, IssueWithDependencyMetadata, Statistics, TreeNode,
};
pub use text::{format_issue_line, format_priority, format_status_icon, format_type_badge};
