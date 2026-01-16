//! Text formatting functions for `beads_rust`.
//!
//! Provides plain text (non-ANSI) formatting for terminal output:
//! - Status icons (○ ◐ ● ❄ ✓ ✗ 📌)
//! - Priority labels (P0-P4)
//! - Type badges ([bug], [feature], etc.)
//! - Issue line formatting

use crate::model::{Issue, IssueType, Priority, Status};

/// Status icon characters.
pub mod icons {
    /// Open issue - available to work (hollow circle).
    pub const OPEN: &str = "○";
    /// In progress - active work (half-filled).
    pub const IN_PROGRESS: &str = "◐";
    /// Blocked - needs attention (filled circle).
    pub const BLOCKED: &str = "●";
    /// Deferred - scheduled for later (snowflake).
    pub const DEFERRED: &str = "❄";
    /// Closed - completed (checkmark).
    pub const CLOSED: &str = "✓";
    /// Tombstone - soft deleted (X mark).
    pub const TOMBSTONE: &str = "✗";
    /// Pinned - elevated priority (pushpin).
    pub const PINNED: &str = "📌";
    /// Unknown status.
    pub const UNKNOWN: &str = "?";
}

/// Return the icon character for a status.
#[must_use]
pub const fn format_status_icon(status: &Status) -> &'static str {
    match status {
        Status::Open => icons::OPEN,
        Status::InProgress => icons::IN_PROGRESS,
        Status::Blocked => icons::BLOCKED,
        Status::Deferred => icons::DEFERRED,
        Status::Closed => icons::CLOSED,
        Status::Tombstone => icons::TOMBSTONE,
        Status::Pinned => icons::PINNED,
        Status::Custom(_) => icons::UNKNOWN,
    }
}

/// Format priority as "P0", "P1", etc.
#[must_use]
pub fn format_priority(priority: &Priority) -> String {
    format!("P{}", priority.0)
}

/// Format issue type as a bracketed badge.
#[must_use]
pub fn format_type_badge(issue_type: &IssueType) -> String {
    format!("[{}]", issue_type.as_str())
}

/// Format a single-line issue summary.
///
/// Format: `{icon} {id} [{priority}] [{type}] {title}`
#[must_use]
pub fn format_issue_line(issue: &Issue) -> String {
    format!(
        "{} {} [{}] {} {}",
        format_status_icon(&issue.status),
        issue.id,
        format_priority(&issue.priority),
        format_type_badge(&issue.issue_type),
        issue.title,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_test_issue() -> Issue {
        Issue {
            id: "bd-test".to_string(),
            content_hash: None,
            title: "Test title".to_string(),
            description: None,
            design: None,
            acceptance_criteria: None,
            notes: None,
            status: Status::Open,
            priority: Priority::MEDIUM,
            issue_type: IssueType::Task,
            assignee: None,
            owner: None,
            estimated_minutes: None,
            created_at: Utc::now(),
            created_by: None,
            updated_at: Utc::now(),
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
        }
    }

    #[test]
    fn test_status_icons() {
        assert_eq!(format_status_icon(&Status::Open), "○");
        assert_eq!(format_status_icon(&Status::InProgress), "◐");
        assert_eq!(format_status_icon(&Status::Blocked), "●");
        assert_eq!(format_status_icon(&Status::Deferred), "❄");
        assert_eq!(format_status_icon(&Status::Closed), "✓");
        assert_eq!(format_status_icon(&Status::Tombstone), "✗");
        assert_eq!(format_status_icon(&Status::Pinned), "📌");
        assert_eq!(
            format_status_icon(&Status::Custom("custom".to_string())),
            "?"
        );
    }

    #[test]
    fn test_format_priority() {
        assert_eq!(format_priority(&Priority::CRITICAL), "P0");
        assert_eq!(format_priority(&Priority::HIGH), "P1");
        assert_eq!(format_priority(&Priority::MEDIUM), "P2");
        assert_eq!(format_priority(&Priority::LOW), "P3");
        assert_eq!(format_priority(&Priority::BACKLOG), "P4");
    }

    #[test]
    fn test_format_type_badge() {
        assert_eq!(format_type_badge(&IssueType::Task), "[task]");
        assert_eq!(format_type_badge(&IssueType::Bug), "[bug]");
        assert_eq!(format_type_badge(&IssueType::Feature), "[feature]");
        assert_eq!(format_type_badge(&IssueType::Epic), "[epic]");
        assert_eq!(format_type_badge(&IssueType::Chore), "[chore]");
        assert_eq!(format_type_badge(&IssueType::Docs), "[docs]");
        assert_eq!(format_type_badge(&IssueType::Question), "[question]");
        assert_eq!(
            format_type_badge(&IssueType::Custom("custom".to_string())),
            "[custom]"
        );
    }

    #[test]
    fn test_format_issue_line_open() {
        let issue = make_test_issue();
        let line = format_issue_line(&issue);
        assert_eq!(line, "○ bd-test [P2] [task] Test title");
    }

    #[test]
    fn test_format_issue_line_in_progress() {
        let mut issue = make_test_issue();
        issue.status = Status::InProgress;
        let line = format_issue_line(&issue);
        assert!(line.starts_with("◐"));
    }

    #[test]
    fn test_format_issue_line_closed() {
        let mut issue = make_test_issue();
        issue.status = Status::Closed;
        let line = format_issue_line(&issue);
        assert!(line.starts_with("✓"));
    }

    #[test]
    fn test_format_issue_line_bug_high_priority() {
        let mut issue = make_test_issue();
        issue.issue_type = IssueType::Bug;
        issue.priority = Priority::HIGH;
        issue.title = "Critical bug".to_string();
        let line = format_issue_line(&issue);
        assert!(line.contains("[P1]"));
        assert!(line.contains("[bug]"));
        assert!(line.contains("Critical bug"));
    }

    #[test]
    fn test_format_issue_line_epic() {
        let mut issue = make_test_issue();
        issue.issue_type = IssueType::Epic;
        issue.priority = Priority::CRITICAL;
        let line = format_issue_line(&issue);
        assert!(line.contains("[P0]"));
        assert!(line.contains("[epic]"));
    }

    #[test]
    fn test_format_issue_line_blocked() {
        let mut issue = make_test_issue();
        issue.status = Status::Blocked;
        let line = format_issue_line(&issue);
        assert!(line.starts_with("●"));
    }

    #[test]
    fn test_format_issue_line_deferred() {
        let mut issue = make_test_issue();
        issue.status = Status::Deferred;
        let line = format_issue_line(&issue);
        assert!(line.starts_with("❄"));
    }
}
