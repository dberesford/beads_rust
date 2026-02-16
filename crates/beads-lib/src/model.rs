//! Core data types for beads-lib.
//!
//! Same serde format as `beads_rust` so JSONL files are interoperable.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;

#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_false(b: &bool) -> bool {
    !*b
}

/// Serialize `Option<i32>` as 0 when None (for bd conformance).
#[allow(clippy::ref_option, clippy::trivially_copy_pass_by_ref)]
fn serialize_compaction_level<S>(value: &Option<i32>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_i32(value.unwrap_or(0))
}

/// Issue lifecycle status.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    #[default]
    Open,
    InProgress,
    Blocked,
    Deferred,
    Closed,
    #[serde(rename = "tombstone")]
    Tombstone,
    #[serde(rename = "pinned")]
    Pinned,
    #[serde(untagged)]
    Custom(String),
}

impl Status {
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Open => "open",
            Self::InProgress => "in_progress",
            Self::Blocked => "blocked",
            Self::Deferred => "deferred",
            Self::Closed => "closed",
            Self::Tombstone => "tombstone",
            Self::Pinned => "pinned",
            Self::Custom(value) => value,
        }
    }

    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Closed | Self::Tombstone)
    }

    #[must_use]
    pub const fn is_active(&self) -> bool {
        matches!(self, Self::Open | Self::InProgress)
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for Status {
    type Err = crate::error::BeadsError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "open" => Ok(Self::Open),
            "in_progress" | "inprogress" => Ok(Self::InProgress),
            "blocked" => Ok(Self::Blocked),
            "deferred" => Ok(Self::Deferred),
            "closed" => Ok(Self::Closed),
            "tombstone" => Ok(Self::Tombstone),
            "pinned" => Ok(Self::Pinned),
            other => Err(crate::error::BeadsError::InvalidStatus {
                status: other.to_string(),
            }),
        }
    }
}

/// Issue priority (0=Critical, 4=Backlog).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(transparent)]
pub struct Priority(pub i32);

impl Priority {
    pub const CRITICAL: Self = Self(0);
    pub const HIGH: Self = Self(1);
    pub const MEDIUM: Self = Self(2);
    pub const LOW: Self = Self(3);
    pub const BACKLOG: Self = Self(4);
}

impl fmt::Display for Priority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "P{}", self.0)
    }
}

impl FromStr for Priority {
    type Err = crate::error::BeadsError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim().to_uppercase();
        let val = s.strip_prefix('P').unwrap_or(&s);

        match val.parse::<i32>() {
            Ok(p) if (0..=4).contains(&p) => Ok(Self(p)),
            Ok(p) => Err(crate::error::BeadsError::InvalidPriority { priority: p }),
            Err(_) => Err(crate::error::BeadsError::InvalidPriority { priority: -1 }),
        }
    }
}

/// Issue type category.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum IssueType {
    #[default]
    Task,
    Bug,
    Feature,
    Epic,
    Chore,
    Docs,
    Question,
    #[serde(untagged)]
    Custom(String),
}

impl IssueType {
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Task => "task",
            Self::Bug => "bug",
            Self::Feature => "feature",
            Self::Epic => "epic",
            Self::Chore => "chore",
            Self::Docs => "docs",
            Self::Question => "question",
            Self::Custom(value) => value,
        }
    }

    #[must_use]
    pub const fn is_standard(&self) -> bool {
        !matches!(self, Self::Custom(_))
    }
}

impl fmt::Display for IssueType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for IssueType {
    type Err = crate::error::BeadsError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "task" => Ok(Self::Task),
            "bug" => Ok(Self::Bug),
            "feature" => Ok(Self::Feature),
            "epic" => Ok(Self::Epic),
            "chore" => Ok(Self::Chore),
            "docs" => Ok(Self::Docs),
            "question" => Ok(Self::Question),
            other => Ok(Self::Custom(other.to_string())),
        }
    }
}

/// Dependency relationship type.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DependencyType {
    Blocks,
    ParentChild,
    ConditionalBlocks,
    WaitsFor,
    Related,
    DiscoveredFrom,
    RepliesTo,
    RelatesTo,
    Duplicates,
    Supersedes,
    CausedBy,
    #[serde(untagged)]
    Custom(String),
}

impl DependencyType {
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Blocks => "blocks",
            Self::ParentChild => "parent-child",
            Self::ConditionalBlocks => "conditional-blocks",
            Self::WaitsFor => "waits-for",
            Self::Related => "related",
            Self::DiscoveredFrom => "discovered-from",
            Self::RepliesTo => "replies-to",
            Self::RelatesTo => "relates-to",
            Self::Duplicates => "duplicates",
            Self::Supersedes => "supersedes",
            Self::CausedBy => "caused-by",
            Self::Custom(value) => value,
        }
    }

    #[must_use]
    pub const fn is_blocking(&self) -> bool {
        matches!(
            self,
            Self::Blocks | Self::ParentChild | Self::ConditionalBlocks | Self::WaitsFor
        )
    }
}

impl fmt::Display for DependencyType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for DependencyType {
    type Err = crate::error::BeadsError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "blocks" => Ok(Self::Blocks),
            "parent-child" => Ok(Self::ParentChild),
            "conditional-blocks" => Ok(Self::ConditionalBlocks),
            "waits-for" => Ok(Self::WaitsFor),
            "related" => Ok(Self::Related),
            "discovered-from" => Ok(Self::DiscoveredFrom),
            "replies-to" => Ok(Self::RepliesTo),
            "relates-to" => Ok(Self::RelatesTo),
            "duplicates" => Ok(Self::Duplicates),
            "supersedes" => Ok(Self::Supersedes),
            "caused-by" => Ok(Self::CausedBy),
            other => Ok(Self::Custom(other.to_string())),
        }
    }
}

/// Audit event type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EventType {
    Created,
    Updated,
    StatusChanged,
    PriorityChanged,
    AssigneeChanged,
    Commented,
    Closed,
    Reopened,
    DependencyAdded,
    DependencyRemoved,
    LabelAdded,
    LabelRemoved,
    Compacted,
    Deleted,
    Restored,
    Custom(String),
}

impl EventType {
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Created => "created",
            Self::Updated => "updated",
            Self::StatusChanged => "status_changed",
            Self::PriorityChanged => "priority_changed",
            Self::AssigneeChanged => "assignee_changed",
            Self::Commented => "commented",
            Self::Closed => "closed",
            Self::Reopened => "reopened",
            Self::DependencyAdded => "dependency_added",
            Self::DependencyRemoved => "dependency_removed",
            Self::LabelAdded => "label_added",
            Self::LabelRemoved => "label_removed",
            Self::Compacted => "compacted",
            Self::Deleted => "deleted",
            Self::Restored => "restored",
            Self::Custom(value) => value,
        }
    }
}

impl Serialize for EventType {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for EventType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        let event_type = match value.as_str() {
            "created" => Self::Created,
            "updated" => Self::Updated,
            "status_changed" => Self::StatusChanged,
            "priority_changed" => Self::PriorityChanged,
            "assignee_changed" => Self::AssigneeChanged,
            "commented" => Self::Commented,
            "closed" => Self::Closed,
            "reopened" => Self::Reopened,
            "dependency_added" => Self::DependencyAdded,
            "dependency_removed" => Self::DependencyRemoved,
            "label_added" => Self::LabelAdded,
            "label_removed" => Self::LabelRemoved,
            "compacted" => Self::Compacted,
            "deleted" => Self::Deleted,
            "restored" => Self::Restored,
            _ => Self::Custom(value),
        };
        Ok(event_type)
    }
}

impl fmt::Display for EventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// The primary issue entity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Issue {
    /// Unique ID (e.g., "bd-abc123").
    pub id: String,

    /// Content hash for deduplication and sync.
    #[serde(skip)]
    pub content_hash: Option<String>,

    /// Title (1-500 chars).
    pub title: String,

    /// Detailed description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Technical design notes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub design: Option<String>,

    /// Acceptance criteria.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acceptance_criteria: Option<String>,

    /// Additional notes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,

    /// Workflow status.
    #[serde(default)]
    pub status: Status,

    /// Priority (0=Critical, 4=Backlog).
    #[serde(default)]
    pub priority: Priority,

    /// Issue type (bug, feature, etc.).
    #[serde(default)]
    pub issue_type: IssueType,

    /// Assigned user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,

    /// Issue owner.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,

    /// Estimated effort in minutes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimated_minutes: Option<i32>,

    /// Creation timestamp.
    pub created_at: DateTime<Utc>,

    /// Creator username.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,

    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,

    /// Closure timestamp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<DateTime<Utc>>,

    /// Reason for closure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub close_reason: Option<String>,

    /// Session ID that closed this issue.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_by_session: Option<String>,

    /// Due date.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due_at: Option<DateTime<Utc>>,

    /// Defer until date.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub defer_until: Option<DateTime<Utc>>,

    /// External reference (e.g., JIRA-123).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_ref: Option<String>,

    /// Source system for imported issues.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_system: Option<String>,

    /// Source repository for multi-repo support.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_repo: Option<String>,

    // Tombstone fields
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delete_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_type: Option<String>,

    // Compaction (legacy/compat)
    #[serde(default, serialize_with = "serialize_compaction_level")]
    pub compaction_level: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compacted_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compacted_at_commit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_size: Option<i32>,

    // Messaging
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub ephemeral: bool,

    // Context
    #[serde(default, skip_serializing_if = "is_false")]
    pub pinned: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_template: bool,

    // Relations (embedded in JSONL, separate tables in SQLite)
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub labels: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub dependencies: Vec<Dependency>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub comments: Vec<Comment>,
}

impl Default for Issue {
    fn default() -> Self {
        Self {
            id: String::new(),
            content_hash: None,
            title: String::new(),
            description: None,
            design: None,
            acceptance_criteria: None,
            notes: None,
            status: Status::default(),
            priority: Priority::default(),
            issue_type: IssueType::default(),
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
            source_repo: None,
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
            labels: Vec::new(),
            dependencies: Vec::new(),
            comments: Vec::new(),
        }
    }
}

impl Issue {
    /// Compute the deterministic content hash for this issue.
    #[must_use]
    pub fn compute_content_hash(&self) -> String {
        crate::util::content_hash(self)
    }
}

/// Relationship between two issues.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Dependency {
    /// The issue that has the dependency (source).
    pub issue_id: String,

    /// The issue being depended on (target).
    pub depends_on_id: String,

    /// Type of dependency.
    #[serde(rename = "type")]
    pub dep_type: DependencyType,

    /// Creation timestamp.
    pub created_at: DateTime<Utc>,

    /// Creator.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,

    /// Optional metadata (JSON).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<String>,

    /// Thread ID for conversation linking.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
}

/// A comment on an issue.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Comment {
    pub id: i64,
    pub issue_id: String,
    pub author: String,
    #[serde(rename = "text")]
    pub body: String,
    pub created_at: DateTime<Utc>,
}

/// An event in the issue's history (audit log).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Event {
    pub id: i64,
    pub issue_id: String,
    pub event_type: EventType,
    pub actor: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    pub created_at: DateTime<Utc>,
}
