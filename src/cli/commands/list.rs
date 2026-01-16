//! List command implementation.
//!
//! Primary discovery interface with classic filter semantics and
//! `IssueWithCounts` JSON output.

use crate::cli::ListArgs;
use crate::error::{BeadsError, Result};
use crate::format::{IssueWithCounts, format_issue_line};
use crate::model::{IssueType, Priority, Status};
use crate::storage::{ListFilters, SqliteStorage};
use std::path::Path;

/// Execute the list command.
///
/// # Errors
///
/// Returns an error if the database cannot be opened or the query fails.
pub fn execute(args: &ListArgs, json: bool) -> Result<()> {
    // Open storage
    let beads_dir = Path::new(".beads");
    if !beads_dir.exists() {
        return Err(BeadsError::NotInitialized);
    }
    let db_path = beads_dir.join("beads.db");
    let storage = SqliteStorage::open(&db_path)?;

    // Build filter from args
    let filters = build_filters(args);

    // Query issues
    let issues = storage.list_issues(&filters)?;

    // Convert to IssueWithCounts
    let issues_with_counts: Vec<IssueWithCounts> = issues
        .into_iter()
        .map(|issue| {
            let dependency_count = storage.count_dependencies(&issue.id).unwrap_or(0);
            let dependent_count = storage.count_dependents(&issue.id).unwrap_or(0);
            IssueWithCounts {
                issue,
                dependency_count,
                dependent_count,
            }
        })
        .collect();

    // Output
    if json {
        let json_output = serde_json::to_string_pretty(&issues_with_counts)?;
        println!("{json_output}");
    } else if issues_with_counts.is_empty() {
        println!("No issues found.");
    } else {
        for iwc in &issues_with_counts {
            let line = format_issue_line(&iwc.issue);
            println!("{line}");
        }
        println!("\n{} issue(s)", issues_with_counts.len());
    }

    Ok(())
}

/// Convert CLI args to storage filter.
fn build_filters(args: &ListArgs) -> ListFilters {
    // Parse status strings to Status enums
    let statuses = if args.status.is_empty() {
        None
    } else {
        let parsed: Vec<Status> = args.status.iter().filter_map(|s| s.parse().ok()).collect();
        if parsed.is_empty() {
            None
        } else {
            Some(parsed)
        }
    };

    // Parse type strings to IssueType enums
    let types = if args.type_.is_empty() {
        None
    } else {
        let parsed: Vec<IssueType> = args.type_.iter().filter_map(|t| t.parse().ok()).collect();
        if parsed.is_empty() {
            None
        } else {
            Some(parsed)
        }
    };

    // Parse priority values
    let priorities = if args.priority.is_empty() {
        None
    } else {
        let parsed: Vec<Priority> = args
            .priority
            .iter()
            .map(|&p| Priority(i32::from(p)))
            .collect();
        Some(parsed)
    };

    ListFilters {
        statuses,
        types,
        priorities,
        assignee: args.assignee.clone(),
        unassigned: args.unassigned,
        include_closed: args.all,
        include_templates: false,
        title_contains: args.title_contains.clone(),
        limit: args.limit,
    }
}
