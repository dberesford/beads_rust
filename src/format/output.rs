use crate::model::{Comment, Event, Issue, Priority, Status};
use serde::{Deserialize, Serialize};

/// Issue with counts for list/search views.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueWithCounts {
    #[serde(flatten)]
    pub issue: Issue,
    pub dependency_count: usize,
    pub dependent_count: usize,
}

/// Issue details with full relations for show view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueDetails {
    #[serde(flatten)]
    pub issue: Issue,
    pub labels: Vec<String>,
    pub dependencies: Vec<IssueWithDependencyMetadata>,
    pub dependents: Vec<IssueWithDependencyMetadata>,
    pub comments: Vec<Comment>,
    pub events: Vec<Event>,
    pub parent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueWithDependencyMetadata {
    pub id: String,
    pub title: String,
    pub status: Status,
    pub priority: Priority,
    pub dep_type: String,
}

/// Blocked issue for blocked view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockedIssue {
    #[serde(flatten)]
    pub issue: Issue,
    pub blocked_by_count: usize,
    pub blocked_by: Vec<String>,
}

/// Tree node for dependency tree view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeNode {
    #[serde(flatten)]
    pub issue: Issue,
    pub depth: usize,
    pub parent_id: Option<String>,
    pub truncated: bool,
}

/// Aggregate statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Statistics {
    // TODO: Define stats structure
    pub total: usize,
}
