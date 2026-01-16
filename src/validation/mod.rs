//! Validation helpers for `beads_rust`.
//!
//! These routines enforce classic bd data constraints and return
//! structured validation errors without mutating storage.

use crate::error::{BeadsError, ValidationError};
use crate::model::{Comment, Dependency, Issue, Priority};

/// Validates issue fields and invariants.
pub struct IssueValidator;

impl IssueValidator {
    /// Validate an issue and return all validation errors found.
    ///
    /// # Errors
    ///
    /// Returns a `Vec<ValidationError>` if any validation rules are violated.
    pub fn validate(issue: &Issue) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();

        // ID: Required, max 50 chars, prefix-hash format.
        if issue.id.trim().is_empty() {
            errors.push(ValidationError::new("id", "cannot be empty"));
        }
        if issue.id.len() > 50 {
            errors.push(ValidationError::new("id", "exceeds 50 characters"));
        }
        if !issue.id.is_empty() && !is_valid_id_format(&issue.id) {
            errors.push(ValidationError::new(
                "id",
                "invalid format (expected prefix-hash)",
            ));
        }

        // Title: Required, max 500 chars.
        if issue.title.trim().is_empty() {
            errors.push(ValidationError::new("title", "cannot be empty"));
        }
        if issue.title.len() > 500 {
            errors.push(ValidationError::new("title", "exceeds 500 characters"));
        }

        // Description: Optional, max 100KB.
        if let Some(description) = issue.description.as_ref() {
            if description.len() > 102_400 {
                errors.push(ValidationError::new("description", "exceeds 100KB"));
            }
        }

        // Priority: 0-4 range.
        if issue.priority.0 < Priority::CRITICAL.0 || issue.priority.0 > Priority::BACKLOG.0 {
            errors.push(ValidationError::new("priority", "must be 0-4"));
        }

        // Timestamps: created_at <= updated_at.
        if issue.updated_at < issue.created_at {
            errors.push(ValidationError::new(
                "updated_at",
                "cannot be before created_at",
            ));
        }

        // External reference: Optional, max 200 chars, no whitespace.
        if let Some(external_ref) = issue.external_ref.as_ref() {
            if external_ref.len() > 200 {
                errors.push(ValidationError::new(
                    "external_ref",
                    "exceeds 200 characters",
                ));
            }
            if external_ref.chars().any(char::is_whitespace) {
                errors.push(ValidationError::new(
                    "external_ref",
                    "cannot contain whitespace",
                ));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// Storage-facing dependency validation helpers.
pub trait DependencyStore {
    /// Return true if the issue exists.
    ///
    /// # Errors
    ///
    /// Returns an error if the storage lookup fails.
    fn issue_exists(&self, id: &str) -> Result<bool, BeadsError>;
    /// Return true if the dependency edge already exists.
    ///
    /// # Errors
    ///
    /// Returns an error if the storage lookup fails.
    fn dependency_exists(&self, issue_id: &str, depends_on_id: &str) -> Result<bool, BeadsError>;
    /// Return true if adding the dependency would create a cycle.
    ///
    /// # Errors
    ///
    /// Returns an error if the storage lookup fails.
    fn would_create_cycle(&self, issue_id: &str, depends_on_id: &str) -> Result<bool, BeadsError>;
}

/// Validates dependency invariants, optionally consulting storage.
pub struct DependencyValidator;

impl DependencyValidator {
    /// Validate dependency rules, returning a `BeadsError` on storage failures.
    ///
    /// # Errors
    ///
    /// Returns a `BeadsError` if storage lookups fail or validation fails.
    pub fn validate(dep: &Dependency, store: &impl DependencyStore) -> Result<(), BeadsError> {
        let mut errors = Vec::new();

        if dep.issue_id == dep.depends_on_id {
            errors.push(ValidationError::new(
                "depends_on_id",
                "issue cannot depend on itself",
            ));
        }

        if !store.issue_exists(&dep.issue_id)? {
            errors.push(ValidationError::new("issue_id", "issue not found"));
        }

        if !store.issue_exists(&dep.depends_on_id)? {
            errors.push(ValidationError::new(
                "depends_on_id",
                "dependency target not found",
            ));
        }

        if store.would_create_cycle(&dep.issue_id, &dep.depends_on_id)? {
            errors.push(ValidationError::new(
                "depends_on_id",
                "would create dependency cycle",
            ));
        }

        if store.dependency_exists(&dep.issue_id, &dep.depends_on_id)? {
            errors.push(ValidationError::new(
                "depends_on_id",
                "dependency already exists",
            ));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(BeadsError::from_validation_errors(errors))
        }
    }
}

/// Validates a single label value.
pub struct LabelValidator;

impl LabelValidator {
    /// Validate a label for length and allowed characters.
    ///
    /// # Errors
    ///
    /// Returns a `ValidationError` if the label is invalid.
    pub fn validate(label: &str) -> Result<(), ValidationError> {
        if label.is_empty() {
            return Err(ValidationError::new("label", "cannot be empty"));
        }

        if label.len() > 50 {
            return Err(ValidationError::new("label", "exceeds 50 characters"));
        }

        if !label
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(ValidationError::new(
                "label",
                "invalid characters (only alphanumeric, hyphen, underscore allowed)",
            ));
        }

        Ok(())
    }
}

/// Validates comment fields.
pub struct CommentValidator;

impl CommentValidator {
    /// Validate a comment and return all validation errors found.
    ///
    /// # Errors
    ///
    /// Returns a `Vec<ValidationError>` if any validation rules are violated.
    pub fn validate(comment: &Comment) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();

        if comment.id.trim().is_empty() {
            errors.push(ValidationError::new("id", "cannot be empty"));
        }

        if comment.issue_id.trim().is_empty() {
            errors.push(ValidationError::new("issue_id", "cannot be empty"));
        }

        if comment.body.trim().is_empty() {
            errors.push(ValidationError::new("content", "cannot be empty"));
        }

        if comment.body.len() > 51_200 {
            errors.push(ValidationError::new("content", "exceeds 50KB"));
        }

        if comment.author.trim().is_empty() {
            errors.push(ValidationError::new("author", "cannot be empty"));
        }

        if comment.author.len() > 200 {
            errors.push(ValidationError::new("author", "exceeds 200 characters"));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

#[must_use]
pub fn is_valid_id_format(id: &str) -> bool {
    let mut parts = id.splitn(2, '-');
    let Some(prefix) = parts.next() else {
        return false;
    };
    let Some(hash) = parts.next() else {
        return false;
    };

    if prefix.is_empty() || prefix.len() > 10 {
        return false;
    }

    if !prefix
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    {
        return false;
    }

    if hash.len() < 3 || hash.len() > 8 {
        return false;
    }

    if !hash
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DependencyType, IssueType, Status};
    use chrono::{TimeZone, Utc};

    fn base_issue() -> Issue {
        Issue {
            id: "bd-abc123".to_string(),
            content_hash: None,
            title: "Test issue".to_string(),
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
            created_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            created_by: None,
            updated_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
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
            labels: Vec::new(),
            dependencies: Vec::new(),
            comments: Vec::new(),
        }
    }

    #[test]
    fn issue_validation_rejects_empty_title() {
        let mut issue = base_issue();
        issue.title = " ".to_string();

        let errors = IssueValidator::validate(&issue).unwrap_err();
        assert!(errors.iter().any(|err| err.field == "title"));
    }

    #[test]
    fn issue_validation_rejects_invalid_id() {
        let mut issue = base_issue();
        issue.id = "invalid".to_string();

        let errors = IssueValidator::validate(&issue).unwrap_err();
        assert!(errors.iter().any(|err| err.field == "id"));
    }

    #[test]
    fn issue_validation_rejects_priority_out_of_range() {
        let mut issue = base_issue();
        issue.priority = Priority(9);

        let errors = IssueValidator::validate(&issue).unwrap_err();
        assert!(errors.iter().any(|err| err.field == "priority"));
    }

    #[test]
    fn label_validation_rejects_invalid_characters() {
        let err = LabelValidator::validate("bad label").unwrap_err();
        assert_eq!(err.field, "label");
    }

    #[test]
    fn comment_validation_rejects_empty_body() {
        let comment = Comment {
            id: "c-1".to_string(),
            issue_id: "bd-abc123".to_string(),
            author: "tester".to_string(),
            body: " ".to_string(),
            created_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        };

        let errors = CommentValidator::validate(&comment).unwrap_err();
        assert!(errors.iter().any(|err| err.field == "content"));
    }

    #[allow(clippy::struct_excessive_bools)]
    struct FakeStore {
        issue_exists: bool,
        depends_on_exists: bool,
        dependency_exists: bool,
        would_cycle: bool,
    }

    impl DependencyStore for FakeStore {
        fn issue_exists(&self, id: &str) -> Result<bool, BeadsError> {
            Ok(match id {
                "issue" => self.issue_exists,
                _ => self.depends_on_exists,
            })
        }

        fn dependency_exists(
            &self,
            _issue_id: &str,
            _depends_on_id: &str,
        ) -> Result<bool, BeadsError> {
            Ok(self.dependency_exists)
        }

        fn would_create_cycle(
            &self,
            _issue_id: &str,
            _depends_on_id: &str,
        ) -> Result<bool, BeadsError> {
            Ok(self.would_cycle)
        }
    }

    fn base_dependency() -> Dependency {
        Dependency {
            issue_id: "issue".to_string(),
            depends_on_id: "dep".to_string(),
            dep_type: DependencyType::Blocks,
            created_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            created_by: None,
            metadata: None,
            thread_id: None,
        }
    }

    #[test]
    fn dependency_validation_rejects_self_dependency() {
        let mut dep = base_dependency();
        dep.depends_on_id = "issue".to_string();
        let store = FakeStore {
            issue_exists: true,
            depends_on_exists: true,
            dependency_exists: false,
            would_cycle: false,
        };

        let err = DependencyValidator::validate(&dep, &store).unwrap_err();
        match err {
            BeadsError::Validation { field, .. } => assert_eq!(field, "depends_on_id"),
            _ => panic!("expected validation error"),
        }
    }

    #[test]
    fn dependency_validation_rejects_missing_issue() {
        let dep = base_dependency();
        let store = FakeStore {
            issue_exists: false,
            depends_on_exists: false,
            dependency_exists: false,
            would_cycle: false,
        };

        let err = DependencyValidator::validate(&dep, &store).unwrap_err();
        match err {
            BeadsError::ValidationErrors { errors } => {
                assert!(errors.iter().any(|e| e.field == "issue_id"));
                assert!(errors.iter().any(|e| e.field == "depends_on_id"));
            }
            _ => panic!("expected validation errors"),
        }
    }

    #[test]
    fn dependency_validation_rejects_cycle() {
        let dep = base_dependency();
        let store = FakeStore {
            issue_exists: true,
            depends_on_exists: true,
            dependency_exists: false,
            would_cycle: true,
        };

        let err = DependencyValidator::validate(&dep, &store).unwrap_err();
        match err {
            BeadsError::Validation { field, .. } => assert_eq!(field, "depends_on_id"),
            _ => panic!("expected validation error"),
        }
    }

    #[test]
    fn dependency_validation_rejects_duplicate() {
        let dep = base_dependency();
        let store = FakeStore {
            issue_exists: true,
            depends_on_exists: true,
            dependency_exists: true,
            would_cycle: false,
        };

        let err = DependencyValidator::validate(&dep, &store).unwrap_err();
        match err {
            BeadsError::Validation { field, .. } => assert_eq!(field, "depends_on_id"),
            _ => panic!("expected validation error"),
        }
    }

    #[test]
    fn issue_validation_collects_multiple_errors() {
        let mut issue = base_issue();
        issue.id = String::new();
        issue.title = String::new();
        issue.priority = Priority(9);
        issue.updated_at = Utc.with_ymd_and_hms(2025, 12, 31, 0, 0, 0).unwrap();

        let errors = IssueValidator::validate(&issue).unwrap_err();
        let fields: Vec<_> = errors.iter().map(|err| err.field.as_str()).collect();
        assert!(fields.contains(&"id"));
        assert!(fields.contains(&"title"));
        assert!(fields.contains(&"priority"));
        assert!(fields.contains(&"updated_at"));
    }

    #[test]
    fn issue_validation_rejects_external_ref_whitespace() {
        let mut issue = base_issue();
        issue.external_ref = Some("gh 12".to_string());

        let errors = IssueValidator::validate(&issue).unwrap_err();
        assert!(errors.iter().any(|err| err.field == "external_ref"));
    }

    #[test]
    fn id_format_validation_accepts_classic_ids() {
        assert!(is_valid_id_format("bd-abc123"));
        assert!(is_valid_id_format("beads9-0a9"));
    }

    #[test]
    fn id_format_validation_rejects_invalid_ids() {
        assert!(!is_valid_id_format("BD-abc123"));
        assert!(!is_valid_id_format("bd-ABC"));
        assert!(!is_valid_id_format("bd-1"));
        assert!(!is_valid_id_format("bd-abc123456"));
        assert!(!is_valid_id_format("bd_abc"));
    }
}
