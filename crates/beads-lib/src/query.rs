//! Query and filter types for issue operations.

use chrono::{DateTime, Utc};

use crate::model::{IssueType, Priority, Status};

/// Fields to update on an issue.
#[derive(Debug, Clone, Default)]
pub struct IssueUpdate {
    pub title: Option<String>,
    pub description: Option<Option<String>>,
    pub design: Option<Option<String>>,
    pub acceptance_criteria: Option<Option<String>>,
    pub notes: Option<Option<String>>,
    pub status: Option<Status>,
    pub priority: Option<Priority>,
    pub issue_type: Option<IssueType>,
    pub assignee: Option<Option<String>>,
    pub owner: Option<Option<String>>,
    pub estimated_minutes: Option<Option<i32>>,
    pub due_at: Option<Option<DateTime<Utc>>>,
    pub defer_until: Option<Option<DateTime<Utc>>>,
    pub external_ref: Option<Option<String>>,
    pub closed_at: Option<Option<DateTime<Utc>>>,
    pub close_reason: Option<Option<String>>,
    pub closed_by_session: Option<Option<String>>,
    pub deleted_at: Option<Option<DateTime<Utc>>>,
    pub deleted_by: Option<Option<String>>,
    pub delete_reason: Option<Option<String>>,
}

impl IssueUpdate {
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.description.is_none()
            && self.design.is_none()
            && self.acceptance_criteria.is_none()
            && self.notes.is_none()
            && self.status.is_none()
            && self.priority.is_none()
            && self.issue_type.is_none()
            && self.assignee.is_none()
            && self.owner.is_none()
            && self.estimated_minutes.is_none()
            && self.due_at.is_none()
            && self.defer_until.is_none()
            && self.external_ref.is_none()
            && self.closed_at.is_none()
            && self.close_reason.is_none()
            && self.closed_by_session.is_none()
            && self.deleted_at.is_none()
            && self.deleted_by.is_none()
            && self.delete_reason.is_none()
    }
}

/// Filter options for listing issues.
#[derive(Debug, Clone, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct ListFilters {
    pub statuses: Option<Vec<Status>>,
    pub types: Option<Vec<IssueType>>,
    pub priorities: Option<Vec<Priority>>,
    pub assignee: Option<String>,
    pub unassigned: bool,
    pub include_closed: bool,
    pub include_deferred: bool,
    pub include_templates: bool,
    pub title_contains: Option<String>,
    pub limit: Option<usize>,
    /// Sort field (priority, created_at, updated_at, title)
    pub sort: Option<String>,
    /// Reverse sort order
    pub reverse: bool,
    /// Filter by labels (all specified labels must match)
    pub labels: Option<Vec<String>>,
    /// Filter by labels (OR logic)
    pub labels_or: Option<Vec<String>>,
    /// Filter by updated_at <= timestamp
    pub updated_before: Option<DateTime<Utc>>,
    /// Filter by updated_at >= timestamp
    pub updated_after: Option<DateTime<Utc>>,
}

/// Filter options for ready issues.
#[derive(Debug, Clone, Default)]
pub struct ReadyFilters {
    pub assignee: Option<String>,
    pub unassigned: bool,
    pub labels_and: Vec<String>,
    pub labels_or: Vec<String>,
    pub types: Option<Vec<IssueType>>,
    pub priorities: Option<Vec<Priority>>,
    pub include_deferred: bool,
    pub limit: Option<usize>,
    /// Filter to children of this parent issue ID.
    pub parent: Option<String>,
    /// Include all descendants (grandchildren, etc.) not just direct children.
    pub recursive: bool,
}

/// Sort policy for ready issues.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
pub enum ReadySortPolicy {
    /// P0/P1 first by created_at ASC, then others by created_at ASC
    #[default]
    Hybrid,
    /// Sort by priority ASC, then created_at ASC
    Priority,
    /// Sort by created_at ASC only
    Oldest,
}
