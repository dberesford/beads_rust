use crate::cli::QuickArgs;
use crate::error::{BeadsError, Result};
use crate::model::{Issue, IssueType, Priority, Status};
use crate::storage::SqliteStorage;
use crate::util::id::IdGenerator;
use crate::validation::LabelValidator;
use chrono::Utc;
use std::path::Path;
use std::str::FromStr;

fn split_labels(values: &[String]) -> Vec<String> {
    let mut labels = Vec::new();
    for value in values {
        for part in value.split(',') {
            let label = part.trim();
            if !label.is_empty() {
                labels.push(label.to_string());
            }
        }
    }
    labels
}

/// Execute the quick capture command.
///
/// # Errors
///
/// Returns an error if validation fails, the database cannot be opened, or creation fails.
pub fn execute(args: QuickArgs) -> Result<()> {
    let title = args.title.join(" ").trim().to_string();
    if title.is_empty() {
        return Err(BeadsError::validation("title", "cannot be empty"));
    }

    let beads_dir = Path::new(".beads");
    if !beads_dir.exists() {
        return Err(BeadsError::NotInitialized);
    }
    let db_path = beads_dir.join("beads.db");
    let mut storage = SqliteStorage::open(&db_path)?;

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

    let id_gen = IdGenerator::with_defaults();
    let now = Utc::now();
    let count = storage.count_issues()?;

    let id = id_gen.generate(&title, None, None, now, count, |candidate| {
        storage.id_exists(candidate).unwrap_or(false)
    });

    let issue = Issue {
        id,
        title,
        description: None,
        status: Status::Open,
        priority,
        issue_type,
        created_at: now,
        updated_at: now,
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

    let actor = std::env::var("USER").unwrap_or_else(|_| "unknown".to_string());
    storage.create_issue(&issue, &actor)?;

    let labels = split_labels(&args.labels);
    for label in labels {
        if let Err(err) = LabelValidator::validate(&label) {
            eprintln!("Warning: invalid label '{label}': {}", err.message);
            continue;
        }

        if let Err(err) = storage.add_label(&issue.id, &label, &actor) {
            eprintln!("Warning: failed to add label '{label}': {err}");
        }
    }

    println!("{}", issue.id);

    Ok(())
}
