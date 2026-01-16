//! Show command implementation.

use crate::error::{BeadsError, Result};
use crate::storage::SqliteStorage;
use std::path::Path;

/// Execute the show command.
///
/// # Errors
///
/// Returns an error if the database cannot be opened or issues are not found.
pub fn execute(ids: Vec<String>, json: bool) -> Result<()> {
    let beads_dir = Path::new(".beads");
    if !beads_dir.exists() {
        return Err(BeadsError::NotInitialized);
    }
    let db_path = beads_dir.join("beads.db");
    let storage = SqliteStorage::open(&db_path)?;

    // TODO: Handle last-touched logic if ids is empty

    if ids.is_empty() {
        return Err(BeadsError::validation("ids", "no issue IDs provided"));
    }

    let mut details = Vec::new();
    for id in ids {
        // TODO: Resolve ID (partial match)
        if let Some(issue) = storage.get_issue(&id)? {
            // TODO: Fetch relations (labels, deps, comments)
            // For now just basic issue
            details.push(issue);
        } else {
            return Err(BeadsError::IssueNotFound { id });
        }
    }

    if json {
        let output = serde_json::to_string_pretty(&details)?;
        println!("{output}");
    } else {
        for issue in details {
            println!(
                "{} {} [{}] [{}]",
                issue.id, issue.title, issue.priority, issue.status
            );
            if let Some(desc) = &issue.description {
                println!("\n{desc}");
            }
            // TODO: Print relations
            println!("----------------------------------------");
        }
    }

    Ok(())
}
