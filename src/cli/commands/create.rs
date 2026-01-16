use crate::cli::CreateArgs;
use crate::error::{BeadsError, Result};
use crate::model::{Issue, IssueType, Priority, Status};
use crate::storage::SqliteStorage;
use crate::util::id::IdGenerator;
use chrono::Utc;
use std::path::Path;
use std::str::FromStr;

/// Execute the create command.
///
/// # Errors
///
/// Returns an error if validation fails, the database cannot be opened, or the issue cannot be created.
pub fn execute(args: CreateArgs) -> Result<()> {
    // 1. Resolve title
    let title = args
        .title
        .or(args.title_flag)
        .ok_or_else(|| BeadsError::validation("title", "cannot be empty"))?;

    if title.is_empty() {
        return Err(BeadsError::validation("title", "cannot be empty"));
    }

    // 2. Open storage
    let beads_dir = Path::new(".beads");
    if !beads_dir.exists() {
        return Err(BeadsError::NotInitialized);
    }
    let db_path = beads_dir.join("beads.db");
    let mut storage = SqliteStorage::open(&db_path)?;

    // 3. Generate ID
    let id_gen = IdGenerator::with_defaults();
    let now = Utc::now();
    let count = storage.count_issues()?;

    let id = id_gen.generate(
        &title,
        None, // description
        None, // creator
        now,
        count,
        |id| storage.id_exists(id).unwrap_or(false),
    );

    // 4. Parse fields
    let priority = if let Some(p) = args.priority {
        Priority::from_str(&p)?
    } else {
        Priority::MEDIUM
    };

    let issue_type = if let Some(t) = args.type_ {
        IssueType::from_str(&t)?
    } else {
        IssueType::Task
    };

    // 5. Construct Issue
    let issue = Issue {
        id,
        title,
        description: args.description,
        status: Status::Open,
        priority,
        issue_type,
        created_at: now,
        updated_at: now,
        // Defaults
        content_hash: None,
        design: None,
        acceptance_criteria: None,
        notes: None,
        assignee: None,
        owner: None,
        estimated_minutes: None,
        created_by: None,
        closed_at: None,
        close_reason: None,
        closed_by_session: None,
        due_at: None,
        defer_until: None,
        external_ref: None,
        source_system: None,
        deleted_at: None,
        deleted_by: None,
        delete_reason: None,
        original_type: None,
        compaction_level: None,
        compacted_at: None,
        compacted_at_commit: None,
        original_size: None,
        sender: None,
        ephemeral: false,
        pinned: false,
        is_template: false,
        labels: vec![],
        dependencies: vec![],
        comments: vec![],
    };

    // 6. Create
    // TODO: get real user
    let user = std::env::var("USER").unwrap_or_else(|_| "unknown".to_string());
    storage.create_issue(&issue, &user)?;

    // 7. Output
    println!("Created {}: {}", issue.id, issue.title);

    Ok(())
}
