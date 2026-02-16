//! In-memory issue store backed by `HashMap`.
//!
//! Provides the full CRUD API for issues, dependencies, labels,
//! comments, and events without any database dependency.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use chrono::Utc;

use crate::error::{BeadsError, Result};
use crate::jsonl;
use crate::model::{Comment, Dependency, DependencyType, Event, EventType, Issue, Status};
use crate::query::{IssueUpdate, ListFilters, ReadyFilters, ReadySortPolicy};

/// In-memory beads issue store.
///
/// All data lives in memory. Use `open()` to load from a JSONL file
/// and `save()` to persist back.
pub struct InMemoryStore {
    issues: HashMap<String, Issue>,
    labels: HashMap<String, Vec<String>>,
    dependencies: Vec<Dependency>,
    comments: HashMap<String, Vec<Comment>>,
    events: Vec<Event>,
    dirty_ids: HashSet<String>,
    config: HashMap<String, String>,
    jsonl_path: Option<PathBuf>,
    next_event_id: i64,
    next_comment_id: i64,
    prefix: String,
}

impl InMemoryStore {
    // ========================================================================
    // Lifecycle
    // ========================================================================

    /// Create a new empty store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            issues: HashMap::new(),
            labels: HashMap::new(),
            dependencies: Vec::new(),
            comments: HashMap::new(),
            events: Vec::new(),
            dirty_ids: HashSet::new(),
            config: HashMap::new(),
            jsonl_path: None,
            next_event_id: 1,
            next_comment_id: 1,
            prefix: "bd".to_string(),
        }
    }

    /// Open and load from a JSONL file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let loaded = jsonl::load(path)?;

        let mut store = Self::new();
        store.jsonl_path = Some(path.to_path_buf());

        for issue in loaded.issues {
            store.issues.insert(issue.id.clone(), issue);
        }

        for (issue_id, issue_labels) in loaded.labels {
            store.labels.insert(issue_id, issue_labels);
        }

        store.dependencies = loaded.dependencies;

        for (issue_id, issue_comments) in loaded.comments {
            // Track max comment ID
            for c in &issue_comments {
                if c.id >= store.next_comment_id {
                    store.next_comment_id = c.id + 1;
                }
            }
            store.comments.insert(issue_id, issue_comments);
        }

        // Infer prefix from first issue ID
        if let Some(id) = store.issues.keys().next() {
            if let Some(dash) = id.rfind('-') {
                store.prefix = id[..dash].to_string();
            }
        }

        Ok(store)
    }

    /// Set the ID prefix for new issues.
    pub fn set_prefix(&mut self, prefix: impl Into<String>) {
        self.prefix = prefix.into();
    }

    /// Get the ID prefix.
    #[must_use]
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    /// Save to the file that was opened.
    ///
    /// # Errors
    ///
    /// Returns `Storage` if no file path is set, or `Io` on write failure.
    pub fn save(&self) -> Result<()> {
        let path = self
            .jsonl_path
            .as_ref()
            .ok_or_else(|| BeadsError::Storage("No file path set; use save_to()".to_string()))?;
        self.save_to(path.clone())
    }

    /// Save to a specific file path.
    ///
    /// # Errors
    ///
    /// Returns `Io` on write failure.
    pub fn save_to(&self, path: impl AsRef<Path>) -> Result<()> {
        let issues: Vec<Issue> = self.get_all_issues_for_export();
        let labels: Vec<(String, Vec<String>)> = self.get_all_labels();
        let deps = self.get_all_dependency_records();
        let comments: Vec<(String, Vec<Comment>)> = self.get_all_comments();

        jsonl::save(path.as_ref(), &issues, &labels, &deps, &comments)
    }

    // ========================================================================
    // CRUD
    // ========================================================================

    /// Create a new issue in the store.
    ///
    /// If `issue.id` is empty, a new ID is generated.
    ///
    /// # Errors
    ///
    /// Returns `IdCollision` if the ID already exists,
    /// or `Validation` if the title is empty.
    pub fn create_issue(&mut self, issue: &Issue, actor: &str) -> Result<Issue> {
        if issue.title.trim().is_empty() {
            return Err(BeadsError::validation("title", "cannot be empty"));
        }

        let mut new_issue = issue.clone();
        let now = Utc::now();

        // Generate ID if not provided
        if new_issue.id.is_empty() {
            new_issue.id = crate::util::generate_id(
                &self.prefix,
                &new_issue.title,
                new_issue.description.as_deref(),
                new_issue.created_by.as_deref().or(Some(actor)),
                now,
                self.issues.len(),
                |id| self.issues.contains_key(id),
            );
        } else if self.issues.contains_key(&new_issue.id) {
            return Err(BeadsError::IdCollision {
                id: new_issue.id.clone(),
            });
        }

        new_issue.created_at = now;
        new_issue.updated_at = now;
        if new_issue.created_by.is_none() {
            new_issue.created_by = Some(actor.to_string());
        }

        // Compute content hash
        new_issue.content_hash = Some(new_issue.compute_content_hash());

        // Extract embedded relations
        let issue_labels = std::mem::take(&mut new_issue.labels);
        let issue_deps = std::mem::take(&mut new_issue.dependencies);
        let issue_comments = std::mem::take(&mut new_issue.comments);

        let id = new_issue.id.clone();

        self.issues.insert(id.clone(), new_issue.clone());

        if !issue_labels.is_empty() {
            self.labels.insert(id.clone(), issue_labels);
        }
        self.dependencies.extend(issue_deps);
        if !issue_comments.is_empty() {
            self.comments.insert(id.clone(), issue_comments);
        }

        self.record_event(&id, EventType::Created, actor, None, None);
        self.dirty_ids.insert(id);

        Ok(new_issue)
    }

    /// Update an existing issue.
    ///
    /// # Errors
    ///
    /// Returns `IssueNotFound` if the issue doesn't exist,
    /// or `Validation` if the update is invalid.
    #[allow(clippy::too_many_lines)]
    pub fn update_issue(&mut self, id: &str, update: &IssueUpdate, actor: &str) -> Result<Issue> {
        // Collect events to record after releasing the mutable borrow on issue
        let mut pending_events: Vec<(EventType, Option<String>, Option<String>)> = Vec::new();

        let issue = self
            .issues
            .get_mut(id)
            .ok_or_else(|| BeadsError::IssueNotFound { id: id.to_string() })?;

        let now = Utc::now();

        if let Some(ref title) = update.title {
            if title.trim().is_empty() {
                return Err(BeadsError::validation("title", "cannot be empty"));
            }
            issue.title.clone_from(title);
        }
        if let Some(ref desc) = update.description {
            issue.description.clone_from(desc);
        }
        if let Some(ref design) = update.design {
            issue.design.clone_from(design);
        }
        if let Some(ref ac) = update.acceptance_criteria {
            issue.acceptance_criteria.clone_from(ac);
        }
        if let Some(ref notes) = update.notes {
            issue.notes.clone_from(notes);
        }
        if let Some(ref status) = update.status {
            let old = issue.status.as_str().to_string();
            issue.status = status.clone();
            pending_events.push((
                EventType::StatusChanged,
                Some(old),
                Some(status.as_str().to_string()),
            ));

            if status.is_terminal() && issue.closed_at.is_none() {
                issue.closed_at = Some(now);
                pending_events.push((EventType::Closed, None, None));
            } else if !status.is_terminal() && issue.closed_at.is_some() {
                issue.closed_at = None;
                pending_events.push((EventType::Reopened, None, None));
            }
        }
        if let Some(ref priority) = update.priority {
            let old = issue.priority.to_string();
            issue.priority = *priority;
            pending_events.push((
                EventType::PriorityChanged,
                Some(old),
                Some(priority.to_string()),
            ));
        }
        if let Some(ref issue_type) = update.issue_type {
            issue.issue_type = issue_type.clone();
        }
        if let Some(ref assignee) = update.assignee {
            let old = issue.assignee.clone();
            issue.assignee.clone_from(assignee);
            pending_events.push((EventType::AssigneeChanged, old, assignee.clone()));
        }
        if let Some(ref owner) = update.owner {
            issue.owner.clone_from(owner);
        }
        if let Some(ref est) = update.estimated_minutes {
            issue.estimated_minutes = *est;
        }
        if let Some(ref due) = update.due_at {
            issue.due_at = *due;
        }
        if let Some(ref defer) = update.defer_until {
            issue.defer_until = *defer;
        }
        if let Some(ref ext_ref) = update.external_ref {
            issue.external_ref.clone_from(ext_ref);
        }
        if let Some(ref closed_at) = update.closed_at {
            issue.closed_at = *closed_at;
        }
        if let Some(ref reason) = update.close_reason {
            issue.close_reason.clone_from(reason);
        }
        if let Some(ref session) = update.closed_by_session {
            issue.closed_by_session.clone_from(session);
        }
        if let Some(ref deleted_at) = update.deleted_at {
            issue.deleted_at = *deleted_at;
        }
        if let Some(ref deleted_by) = update.deleted_by {
            issue.deleted_by.clone_from(deleted_by);
        }
        if let Some(ref reason) = update.delete_reason {
            issue.delete_reason.clone_from(reason);
        }

        issue.updated_at = now;
        issue.content_hash = Some(issue.compute_content_hash());

        let updated = issue.clone();

        // Now record all pending events (borrow on issue is released)
        for (event_type, old_value, new_value) in pending_events {
            self.record_event(
                id,
                event_type,
                actor,
                old_value.as_deref(),
                new_value.as_deref(),
            );
        }
        self.record_event(id, EventType::Updated, actor, None, None);
        self.dirty_ids.insert(id.to_string());

        Ok(updated)
    }

    /// Delete an issue from the store.
    ///
    /// # Errors
    ///
    /// Returns `IssueNotFound` if the issue doesn't exist,
    /// or `HasDependents` if other issues depend on it.
    pub fn delete_issue(&mut self, id: &str, actor: &str, force: bool) -> Result<()> {
        if !self.issues.contains_key(id) {
            return Err(BeadsError::IssueNotFound { id: id.to_string() });
        }

        if !force {
            let dependents = self.get_dependents(id);
            if !dependents.is_empty() {
                return Err(BeadsError::HasDependents {
                    id: id.to_string(),
                    count: dependents.len(),
                });
            }
        }

        self.issues.remove(id);
        self.labels.remove(id);
        self.comments.remove(id);
        self.dependencies
            .retain(|d| d.issue_id != id && d.depends_on_id != id);

        self.record_event(id, EventType::Deleted, actor, None, None);
        self.dirty_ids.insert(id.to_string());

        Ok(())
    }

    /// Get a single issue by ID.
    ///
    /// # Errors
    ///
    /// Returns `IssueNotFound` if the issue doesn't exist.
    pub fn get_issue(&self, id: &str) -> Result<&Issue> {
        self.issues
            .get(id)
            .ok_or_else(|| BeadsError::IssueNotFound { id: id.to_string() })
    }

    /// Get multiple issues by their IDs.
    #[must_use]
    pub fn get_issues_by_ids(&self, ids: &[String]) -> Vec<&Issue> {
        ids.iter().filter_map(|id| self.issues.get(id)).collect()
    }

    // ========================================================================
    // Queries
    // ========================================================================

    /// List issues with filters.
    #[must_use]
    pub fn list_issues(&self, filters: &ListFilters) -> Vec<&Issue> {
        let mut results: Vec<&Issue> = self
            .issues
            .values()
            .filter(|issue| self.matches_list_filters(issue, filters))
            .collect();

        Self::sort_issues(&mut results, filters.sort.as_deref(), filters.reverse);

        if let Some(limit) = filters.limit {
            results.truncate(limit);
        }

        results
    }

    /// Search issues by title substring.
    #[must_use]
    pub fn search_issues(&self, query: &str) -> Vec<&Issue> {
        let query_lower = query.to_lowercase();
        self.issues
            .values()
            .filter(|issue| {
                issue.title.to_lowercase().contains(&query_lower)
                    || issue
                        .description
                        .as_ref()
                        .is_some_and(|d| d.to_lowercase().contains(&query_lower))
            })
            .collect()
    }

    /// Get issues that are ready to work on (not blocked).
    #[must_use]
    pub fn get_ready_issues(
        &self,
        filters: &ReadyFilters,
        sort_policy: ReadySortPolicy,
    ) -> Vec<&Issue> {
        let mut results: Vec<&Issue> = self
            .issues
            .values()
            .filter(|issue| self.is_ready_issue(issue, filters))
            .collect();

        match sort_policy {
            ReadySortPolicy::Hybrid => {
                results.sort_by(|a, b| {
                    let a_urgent = a.priority.0 <= 1;
                    let b_urgent = b.priority.0 <= 1;
                    match (a_urgent, b_urgent) {
                        (true, false) => std::cmp::Ordering::Less,
                        (false, true) => std::cmp::Ordering::Greater,
                        _ => a.created_at.cmp(&b.created_at),
                    }
                });
            }
            ReadySortPolicy::Priority => {
                results.sort_by(|a, b| {
                    a.priority
                        .cmp(&b.priority)
                        .then(a.created_at.cmp(&b.created_at))
                });
            }
            ReadySortPolicy::Oldest => {
                results.sort_by_key(|a| a.created_at);
            }
        }

        if let Some(limit) = filters.limit {
            results.truncate(limit);
        }

        results
    }

    /// Get issues that are blocked by dependencies.
    #[must_use]
    pub fn get_blocked_issues(&self) -> Vec<&Issue> {
        self.issues
            .values()
            .filter(|issue| !issue.status.is_terminal() && self.is_blocked(&issue.id))
            .collect()
    }

    /// Count issues matching the given filters.
    #[must_use]
    pub fn count_issues(&self, filters: &ListFilters) -> usize {
        self.issues
            .values()
            .filter(|issue| self.matches_list_filters(issue, filters))
            .count()
    }

    // ========================================================================
    // Dependencies
    // ========================================================================

    /// Add a dependency between two issues.
    ///
    /// # Errors
    ///
    /// Returns `SelfDependency`, `IssueNotFound`, `DuplicateDependency`,
    /// or `DependencyCycle`.
    pub fn add_dependency(
        &mut self,
        issue_id: &str,
        depends_on_id: &str,
        dep_type: DependencyType,
        actor: &str,
        metadata: Option<String>,
    ) -> Result<()> {
        if issue_id == depends_on_id {
            return Err(BeadsError::SelfDependency {
                id: issue_id.to_string(),
            });
        }

        if !self.issues.contains_key(issue_id) {
            return Err(BeadsError::IssueNotFound {
                id: issue_id.to_string(),
            });
        }
        if !self.issues.contains_key(depends_on_id) {
            return Err(BeadsError::DependencyNotFound {
                id: depends_on_id.to_string(),
            });
        }

        // Check for duplicates
        if self.dependency_exists(issue_id, depends_on_id) {
            return Err(BeadsError::DuplicateDependency {
                from: issue_id.to_string(),
                to: depends_on_id.to_string(),
            });
        }

        // Cycle detection
        if self.would_create_cycle(issue_id, depends_on_id) {
            return Err(BeadsError::DependencyCycle {
                path: format!("{issue_id} -> {depends_on_id}"),
            });
        }

        self.dependencies.push(Dependency {
            issue_id: issue_id.to_string(),
            depends_on_id: depends_on_id.to_string(),
            dep_type,
            created_at: Utc::now(),
            created_by: Some(actor.to_string()),
            metadata,
            thread_id: None,
        });

        self.record_event(
            issue_id,
            EventType::DependencyAdded,
            actor,
            None,
            Some(depends_on_id),
        );
        self.dirty_ids.insert(issue_id.to_string());

        Ok(())
    }

    /// Remove a dependency between two issues.
    ///
    /// # Errors
    ///
    /// Returns `NothingToDo` if the dependency doesn't exist.
    pub fn remove_dependency(
        &mut self,
        issue_id: &str,
        depends_on_id: &str,
        actor: &str,
    ) -> Result<()> {
        let before = self.dependencies.len();
        self.dependencies
            .retain(|d| !(d.issue_id == issue_id && d.depends_on_id == depends_on_id));

        if self.dependencies.len() == before {
            return Err(BeadsError::NothingToDo {
                reason: format!("No dependency from {issue_id} to {depends_on_id}"),
            });
        }

        self.record_event(
            issue_id,
            EventType::DependencyRemoved,
            actor,
            Some(depends_on_id),
            None,
        );
        self.dirty_ids.insert(issue_id.to_string());

        Ok(())
    }

    /// Get all dependencies for an issue (things this issue depends on).
    #[must_use]
    pub fn get_dependencies(&self, issue_id: &str) -> Vec<&Dependency> {
        self.dependencies
            .iter()
            .filter(|d| d.issue_id == issue_id)
            .collect()
    }

    /// Get all issues that depend on the given issue.
    #[must_use]
    pub fn get_dependents(&self, issue_id: &str) -> Vec<&Dependency> {
        self.dependencies
            .iter()
            .filter(|d| d.depends_on_id == issue_id)
            .collect()
    }

    /// Check if an issue is blocked by any dependency.
    #[must_use]
    pub fn is_blocked(&self, issue_id: &str) -> bool {
        self.dependencies.iter().any(|d| {
            d.issue_id == issue_id
                && d.dep_type.is_blocking()
                && self
                    .issues
                    .get(&d.depends_on_id)
                    .is_some_and(|i| !i.status.is_terminal())
        })
    }

    /// Get the issues blocking a given issue.
    #[must_use]
    pub fn get_blockers(&self, issue_id: &str) -> Vec<&Issue> {
        self.dependencies
            .iter()
            .filter(|d| d.issue_id == issue_id && d.dep_type.is_blocking())
            .filter_map(|d| {
                self.issues
                    .get(&d.depends_on_id)
                    .filter(|i| !i.status.is_terminal())
            })
            .collect()
    }

    /// Check if a dependency edge already exists.
    #[must_use]
    pub fn dependency_exists(&self, issue_id: &str, depends_on_id: &str) -> bool {
        self.dependencies
            .iter()
            .any(|d| d.issue_id == issue_id && d.depends_on_id == depends_on_id)
    }

    /// Check if adding a dependency would create a cycle.
    ///
    /// BFS from `depends_on_id` following dependency edges.
    /// Returns true if `issue_id` is reachable.
    #[must_use]
    pub fn would_create_cycle(&self, issue_id: &str, depends_on_id: &str) -> bool {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(depends_on_id.to_string());

        while let Some(current) = queue.pop_front() {
            if current == issue_id {
                return true;
            }
            if !visited.insert(current.clone()) {
                continue;
            }
            for dep in &self.dependencies {
                if dep.issue_id == current {
                    queue.push_back(dep.depends_on_id.clone());
                }
            }
        }

        false
    }

    // ========================================================================
    // Labels
    // ========================================================================

    /// Add a label to an issue.
    ///
    /// # Errors
    ///
    /// Returns `IssueNotFound` if the issue doesn't exist.
    pub fn add_label(&mut self, issue_id: &str, label: &str, actor: &str) -> Result<()> {
        if !self.issues.contains_key(issue_id) {
            return Err(BeadsError::IssueNotFound {
                id: issue_id.to_string(),
            });
        }

        let labels = self.labels.entry(issue_id.to_string()).or_default();
        if !labels.contains(&label.to_string()) {
            labels.push(label.to_string());
            self.record_event(issue_id, EventType::LabelAdded, actor, None, Some(label));
            self.dirty_ids.insert(issue_id.to_string());
        }

        Ok(())
    }

    /// Remove a label from an issue.
    ///
    /// # Errors
    ///
    /// Returns `IssueNotFound` if the issue doesn't exist.
    pub fn remove_label(&mut self, issue_id: &str, label: &str, actor: &str) -> Result<()> {
        if !self.issues.contains_key(issue_id) {
            return Err(BeadsError::IssueNotFound {
                id: issue_id.to_string(),
            });
        }

        if let Some(labels) = self.labels.get_mut(issue_id) {
            if let Some(pos) = labels.iter().position(|l| l == label) {
                labels.remove(pos);
                self.record_event(issue_id, EventType::LabelRemoved, actor, Some(label), None);
                self.dirty_ids.insert(issue_id.to_string());
            }
        }

        Ok(())
    }

    /// Get labels for an issue.
    #[must_use]
    pub fn get_labels(&self, issue_id: &str) -> Vec<&str> {
        self.labels
            .get(issue_id)
            .map(|l| l.iter().map(String::as_str).collect())
            .unwrap_or_default()
    }

    /// Set all labels for an issue (replaces existing).
    ///
    /// # Errors
    ///
    /// Returns `IssueNotFound` if the issue doesn't exist.
    pub fn set_labels(&mut self, issue_id: &str, labels: Vec<String>) -> Result<()> {
        if !self.issues.contains_key(issue_id) {
            return Err(BeadsError::IssueNotFound {
                id: issue_id.to_string(),
            });
        }

        self.labels.insert(issue_id.to_string(), labels);
        self.dirty_ids.insert(issue_id.to_string());
        Ok(())
    }

    /// Get all unique labels with their usage counts.
    #[must_use]
    pub fn get_unique_labels_with_counts(&self) -> Vec<(String, usize)> {
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for labels in self.labels.values() {
            for label in labels {
                *counts.entry(label.as_str()).or_insert(0) += 1;
            }
        }
        let mut result: Vec<(String, usize)> = counts
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        result.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        result
    }

    // ========================================================================
    // Comments
    // ========================================================================

    /// Add a comment to an issue.
    ///
    /// # Errors
    ///
    /// Returns `IssueNotFound` if the issue doesn't exist.
    pub fn add_comment(&mut self, issue_id: &str, author: &str, body: &str) -> Result<Comment> {
        if !self.issues.contains_key(issue_id) {
            return Err(BeadsError::IssueNotFound {
                id: issue_id.to_string(),
            });
        }

        let comment = Comment {
            id: self.next_comment_id,
            issue_id: issue_id.to_string(),
            author: author.to_string(),
            body: body.to_string(),
            created_at: Utc::now(),
        };
        self.next_comment_id += 1;

        self.comments
            .entry(issue_id.to_string())
            .or_default()
            .push(comment.clone());

        self.record_event(issue_id, EventType::Commented, author, None, Some(body));
        self.dirty_ids.insert(issue_id.to_string());

        Ok(comment)
    }

    /// Get comments for an issue.
    #[must_use]
    pub fn get_comments(&self, issue_id: &str) -> Vec<&Comment> {
        self.comments
            .get(issue_id)
            .map(|c| c.iter().collect())
            .unwrap_or_default()
    }

    // ========================================================================
    // Events
    // ========================================================================

    /// Get events for a specific issue.
    #[must_use]
    pub fn get_events(&self, issue_id: &str) -> Vec<&Event> {
        self.events
            .iter()
            .filter(|e| e.issue_id == issue_id)
            .collect()
    }

    /// Get all events across all issues.
    #[must_use]
    pub fn get_all_events(&self) -> &[Event] {
        &self.events
    }

    // ========================================================================
    // ID Resolution
    // ========================================================================

    /// Check if an issue ID exists.
    #[must_use]
    pub fn id_exists(&self, id: &str) -> bool {
        self.issues.contains_key(id)
    }

    /// Find IDs by content hash.
    #[must_use]
    pub fn find_ids_by_hash(&self, hash: &str) -> Vec<String> {
        self.issues
            .values()
            .filter(|i| i.content_hash.as_deref() == Some(hash))
            .map(|i| i.id.clone())
            .collect()
    }

    /// Get all issue IDs.
    #[must_use]
    pub fn get_all_ids(&self) -> Vec<String> {
        self.issues.keys().cloned().collect()
    }

    /// Resolve a partial ID to a full ID.
    ///
    /// Tries: exact match, prefix-normalized, substring match.
    ///
    /// # Errors
    ///
    /// Returns `IssueNotFound` or `AmbiguousId`.
    pub fn resolve_id(&self, input: &str) -> Result<String> {
        let input = input.trim().to_lowercase();

        if input.is_empty() {
            return Err(BeadsError::InvalidId { id: String::new() });
        }

        // Exact match
        if self.issues.contains_key(&input) {
            return Ok(input);
        }

        // Prefix-normalized
        if !input.contains('-') {
            let with_prefix = format!("{}-{}", self.prefix, input);
            if self.issues.contains_key(&with_prefix) {
                return Ok(with_prefix);
            }
        }

        // Substring match on hash portion
        let hash_pattern = input
            .rfind('-')
            .map_or(input.as_str(), |pos| &input[pos + 1..]);

        if !hash_pattern.is_empty() {
            let matches: Vec<String> = self
                .issues
                .keys()
                .filter(|id| {
                    id.rfind('-').is_some_and(|pos| {
                        let hash = &id[pos + 1..];
                        let base = hash.split('.').next().unwrap_or(hash);
                        base.contains(hash_pattern)
                    })
                })
                .cloned()
                .collect();

            match matches.len() {
                0 => {}
                1 => return Ok(matches.into_iter().next().unwrap_or_default()),
                _ => {
                    return Err(BeadsError::AmbiguousId {
                        partial: input,
                        matches,
                    });
                }
            }
        }

        Err(BeadsError::IssueNotFound { id: input })
    }

    // ========================================================================
    // Bulk Export
    // ========================================================================

    /// Get all issues for export (sorted by ID for deterministic output).
    #[must_use]
    pub fn get_all_issues_for_export(&self) -> Vec<Issue> {
        let mut issues: Vec<Issue> = self.issues.values().cloned().collect();
        issues.sort_by(|a, b| a.id.cmp(&b.id));
        issues
    }

    /// Get all dependency records.
    #[must_use]
    pub fn get_all_dependency_records(&self) -> Vec<Dependency> {
        self.dependencies.clone()
    }

    /// Get all comments grouped by issue ID.
    #[must_use]
    pub fn get_all_comments(&self) -> Vec<(String, Vec<Comment>)> {
        self.comments
            .iter()
            .map(|(id, c)| (id.clone(), c.clone()))
            .collect()
    }

    /// Get all labels grouped by issue ID.
    #[must_use]
    pub fn get_all_labels(&self) -> Vec<(String, Vec<String>)> {
        self.labels
            .iter()
            .map(|(id, l)| (id.clone(), l.clone()))
            .collect()
    }

    // ========================================================================
    // Config
    // ========================================================================

    /// Get a configuration value.
    #[must_use]
    pub fn get_config(&self, key: &str) -> Option<&str> {
        self.config.get(key).map(String::as_str)
    }

    /// Set a configuration value.
    pub fn set_config(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.config.insert(key.into(), value.into());
    }

    // ========================================================================
    // Dirty Tracking
    // ========================================================================

    /// Check if any issues have been modified.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        !self.dirty_ids.is_empty()
    }

    /// Get the number of modified issues.
    #[must_use]
    pub fn dirty_count(&self) -> usize {
        self.dirty_ids.len()
    }

    /// Clear dirty tracking flags.
    pub fn clear_dirty(&mut self) {
        self.dirty_ids.clear();
    }

    /// Get the total number of issues.
    #[must_use]
    pub fn len(&self) -> usize {
        self.issues.len()
    }

    /// Check if the store is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.issues.is_empty()
    }

    // ========================================================================
    // Internal Helpers
    // ========================================================================

    fn record_event(
        &mut self,
        issue_id: &str,
        event_type: EventType,
        actor: &str,
        old_value: Option<&str>,
        new_value: Option<&str>,
    ) {
        let event = Event {
            id: self.next_event_id,
            issue_id: issue_id.to_string(),
            event_type,
            actor: actor.to_string(),
            old_value: old_value.map(String::from),
            new_value: new_value.map(String::from),
            comment: None,
            created_at: Utc::now(),
        };
        self.next_event_id += 1;
        self.events.push(event);
    }

    fn matches_list_filters(&self, issue: &Issue, filters: &ListFilters) -> bool {
        // Status filtering
        if let Some(ref statuses) = filters.statuses {
            if !statuses.contains(&issue.status) {
                return false;
            }
        } else {
            // Default: exclude closed and tombstone
            if !filters.include_closed && issue.status.is_terminal() {
                return false;
            }
            if !filters.include_deferred && issue.status == Status::Deferred {
                return false;
            }
        }

        // Template filtering
        if !filters.include_templates && issue.is_template {
            return false;
        }

        // Type filtering
        if let Some(ref types) = filters.types {
            if !types.contains(&issue.issue_type) {
                return false;
            }
        }

        // Priority filtering
        if let Some(ref priorities) = filters.priorities {
            if !priorities.contains(&issue.priority) {
                return false;
            }
        }

        // Assignee filtering
        if filters.unassigned && issue.assignee.is_some() {
            return false;
        }
        if let Some(ref assignee) = filters.assignee {
            if issue.assignee.as_deref() != Some(assignee.as_str()) {
                return false;
            }
        }

        // Title search
        if let Some(ref query) = filters.title_contains {
            if !issue.title.to_lowercase().contains(&query.to_lowercase()) {
                return false;
            }
        }

        // Label filtering (AND)
        if let Some(ref required_labels) = filters.labels {
            let issue_labels = self.get_labels(&issue.id);
            if !required_labels
                .iter()
                .all(|l| issue_labels.contains(&l.as_str()))
            {
                return false;
            }
        }

        // Label filtering (OR)
        if let Some(ref or_labels) = filters.labels_or {
            let issue_labels = self.get_labels(&issue.id);
            if !or_labels.iter().any(|l| issue_labels.contains(&l.as_str())) {
                return false;
            }
        }

        // Timestamp filtering
        if let Some(before) = filters.updated_before {
            if issue.updated_at > before {
                return false;
            }
        }
        if let Some(after) = filters.updated_after {
            if issue.updated_at < after {
                return false;
            }
        }

        true
    }

    fn sort_issues(issues: &mut [&Issue], sort: Option<&str>, reverse: bool) {
        match sort {
            Some("priority") => {
                issues.sort_by_key(|a| a.priority);
            }
            Some("created_at" | "created") => {
                issues.sort_by_key(|a| a.created_at);
            }
            Some("updated_at" | "updated") => {
                issues.sort_by_key(|a| a.updated_at);
            }
            Some("title") => {
                issues.sort_by_key(|a| a.title.to_lowercase());
            }
            _ => {
                // Default: priority ASC, then created_at ASC
                issues.sort_by(|a, b| {
                    a.priority
                        .cmp(&b.priority)
                        .then(a.created_at.cmp(&b.created_at))
                });
            }
        }

        if reverse {
            issues.reverse();
        }
    }

    fn is_ready_issue(&self, issue: &Issue, filters: &ReadyFilters) -> bool {
        // Must be active (open or in_progress)
        if !issue.status.is_active() {
            if filters.include_deferred && issue.status == Status::Deferred {
                // Allow deferred if explicitly requested
            } else {
                return false;
            }
        }

        // Must not be blocked
        if self.is_blocked(&issue.id) {
            return false;
        }

        // Skip templates
        if issue.is_template {
            return false;
        }

        // Assignee filtering
        if filters.unassigned && issue.assignee.is_some() {
            return false;
        }
        if let Some(ref assignee) = filters.assignee {
            if issue.assignee.as_deref() != Some(assignee.as_str()) {
                return false;
            }
        }

        // Type filtering
        if let Some(ref types) = filters.types {
            if !types.contains(&issue.issue_type) {
                return false;
            }
        }

        // Priority filtering
        if let Some(ref priorities) = filters.priorities {
            if !priorities.contains(&issue.priority) {
                return false;
            }
        }

        // Label filtering (AND)
        if !filters.labels_and.is_empty() {
            let issue_labels = self.get_labels(&issue.id);
            if !filters
                .labels_and
                .iter()
                .all(|l| issue_labels.contains(&l.as_str()))
            {
                return false;
            }
        }

        // Label filtering (OR)
        if !filters.labels_or.is_empty() {
            let issue_labels = self.get_labels(&issue.id);
            if !filters
                .labels_or
                .iter()
                .any(|l| issue_labels.contains(&l.as_str()))
            {
                return false;
            }
        }

        // Parent filtering
        if let Some(ref parent) = filters.parent {
            let is_child = if filters.recursive {
                issue.id.starts_with(parent) && issue.id.len() > parent.len()
            } else {
                // Direct child: parent_id.N (one dot after parent)
                issue.id.starts_with(parent)
                    && issue.id.len() > parent.len()
                    && issue.id.as_bytes().get(parent.len()) == Some(&b'.')
                    && !issue.id[parent.len() + 1..].contains('.')
            };
            if !is_child {
                return false;
            }
        }

        true
    }
}

impl Default for InMemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{IssueType, Priority};

    fn make_issue(id: &str, title: &str) -> Issue {
        Issue {
            id: id.to_string(),
            title: title.to_string(),
            status: Status::Open,
            priority: Priority::MEDIUM,
            ..Default::default()
        }
    }

    #[test]
    fn test_create_and_get() {
        let mut store = InMemoryStore::new();
        let issue = make_issue("", "Test issue");
        let created = store.create_issue(&issue, "user").unwrap();
        assert!(!created.id.is_empty());
        assert_eq!(created.title, "Test issue");

        let fetched = store.get_issue(&created.id).unwrap();
        assert_eq!(fetched.title, "Test issue");
    }

    #[test]
    fn test_create_with_explicit_id() {
        let mut store = InMemoryStore::new();
        let issue = make_issue("bd-test1", "Explicit ID");
        let created = store.create_issue(&issue, "user").unwrap();
        assert_eq!(created.id, "bd-test1");
    }

    #[test]
    fn test_create_id_collision() {
        let mut store = InMemoryStore::new();
        let issue = make_issue("bd-test1", "First");
        store.create_issue(&issue, "user").unwrap();

        let dup = make_issue("bd-test1", "Duplicate");
        let result = store.create_issue(&dup, "user");
        assert!(matches!(result, Err(BeadsError::IdCollision { .. })));
    }

    #[test]
    fn test_create_empty_title_rejected() {
        let mut store = InMemoryStore::new();
        let issue = make_issue("", "  ");
        let result = store.create_issue(&issue, "user");
        assert!(matches!(result, Err(BeadsError::Validation { .. })));
    }

    #[test]
    fn test_update_issue() {
        let mut store = InMemoryStore::new();
        let issue = make_issue("bd-upd1", "Original");
        store.create_issue(&issue, "user").unwrap();

        let update = IssueUpdate {
            title: Some("Updated".to_string()),
            status: Some(Status::InProgress),
            ..Default::default()
        };
        let updated = store.update_issue("bd-upd1", &update, "user").unwrap();
        assert_eq!(updated.title, "Updated");
        assert_eq!(updated.status, Status::InProgress);
    }

    #[test]
    fn test_update_nonexistent() {
        let mut store = InMemoryStore::new();
        let update = IssueUpdate {
            title: Some("X".to_string()),
            ..Default::default()
        };
        let result = store.update_issue("bd-nope", &update, "user");
        assert!(matches!(result, Err(BeadsError::IssueNotFound { .. })));
    }

    #[test]
    fn test_delete_issue() {
        let mut store = InMemoryStore::new();
        let issue = make_issue("bd-del1", "Delete me");
        store.create_issue(&issue, "user").unwrap();

        store.delete_issue("bd-del1", "user", false).unwrap();
        assert!(store.get_issue("bd-del1").is_err());
    }

    #[test]
    fn test_delete_with_dependents_blocked() {
        let mut store = InMemoryStore::new();
        store
            .create_issue(&make_issue("bd-a1", "A"), "user")
            .unwrap();
        store
            .create_issue(&make_issue("bd-b1", "B"), "user")
            .unwrap();
        store
            .add_dependency("bd-b1", "bd-a1", DependencyType::Blocks, "user", None)
            .unwrap();

        let result = store.delete_issue("bd-a1", "user", false);
        assert!(matches!(result, Err(BeadsError::HasDependents { .. })));

        // Force delete works
        store.delete_issue("bd-a1", "user", true).unwrap();
        assert!(store.get_issue("bd-a1").is_err());
    }

    #[test]
    fn test_list_filters_status() {
        let mut store = InMemoryStore::new();
        store
            .create_issue(&make_issue("bd-l1", "Open"), "user")
            .unwrap();
        let mut closed = make_issue("bd-l2", "Closed");
        closed.status = Status::Closed;
        store.create_issue(&closed, "user").unwrap();

        let open = store.list_issues(&ListFilters::default());
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].title, "Open");

        let all = store.list_issues(&ListFilters {
            include_closed: true,
            ..Default::default()
        });
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_search_issues() {
        let mut store = InMemoryStore::new();
        store
            .create_issue(&make_issue("bd-s1", "Fix login bug"), "user")
            .unwrap();
        store
            .create_issue(&make_issue("bd-s2", "Add feature"), "user")
            .unwrap();

        let results = store.search_issues("login");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Fix login bug");
    }

    #[test]
    fn test_dependencies_cycle_detection() {
        let mut store = InMemoryStore::new();
        store
            .create_issue(&make_issue("bd-c1", "A"), "user")
            .unwrap();
        store
            .create_issue(&make_issue("bd-c2", "B"), "user")
            .unwrap();
        store
            .create_issue(&make_issue("bd-c3", "C"), "user")
            .unwrap();

        store
            .add_dependency("bd-c1", "bd-c2", DependencyType::Blocks, "user", None)
            .unwrap();
        store
            .add_dependency("bd-c2", "bd-c3", DependencyType::Blocks, "user", None)
            .unwrap();

        // c3 -> c1 would create a cycle
        let result = store.add_dependency("bd-c3", "bd-c1", DependencyType::Blocks, "user", None);
        assert!(matches!(result, Err(BeadsError::DependencyCycle { .. })));
    }

    #[test]
    fn test_self_dependency_rejected() {
        let mut store = InMemoryStore::new();
        store
            .create_issue(&make_issue("bd-sd1", "Self"), "user")
            .unwrap();

        let result = store.add_dependency("bd-sd1", "bd-sd1", DependencyType::Blocks, "user", None);
        assert!(matches!(result, Err(BeadsError::SelfDependency { .. })));
    }

    #[test]
    fn test_duplicate_dependency_rejected() {
        let mut store = InMemoryStore::new();
        store
            .create_issue(&make_issue("bd-dd1", "A"), "user")
            .unwrap();
        store
            .create_issue(&make_issue("bd-dd2", "B"), "user")
            .unwrap();

        store
            .add_dependency("bd-dd1", "bd-dd2", DependencyType::Blocks, "user", None)
            .unwrap();
        let result = store.add_dependency("bd-dd1", "bd-dd2", DependencyType::Blocks, "user", None);
        assert!(matches!(
            result,
            Err(BeadsError::DuplicateDependency { .. })
        ));
    }

    #[test]
    fn test_is_blocked() {
        let mut store = InMemoryStore::new();
        store
            .create_issue(&make_issue("bd-bl1", "Blocker"), "user")
            .unwrap();
        store
            .create_issue(&make_issue("bd-bl2", "Blocked"), "user")
            .unwrap();

        store
            .add_dependency("bd-bl2", "bd-bl1", DependencyType::Blocks, "user", None)
            .unwrap();

        assert!(store.is_blocked("bd-bl2"));
        assert!(!store.is_blocked("bd-bl1"));
    }

    #[test]
    fn test_is_not_blocked_when_blocker_closed() {
        let mut store = InMemoryStore::new();
        let mut blocker = make_issue("bd-bc1", "Blocker");
        blocker.status = Status::Closed;
        store.create_issue(&blocker, "user").unwrap();
        store
            .create_issue(&make_issue("bd-bc2", "Was blocked"), "user")
            .unwrap();

        store
            .add_dependency("bd-bc2", "bd-bc1", DependencyType::Blocks, "user", None)
            .unwrap();

        assert!(!store.is_blocked("bd-bc2"));
    }

    #[test]
    fn test_labels() {
        let mut store = InMemoryStore::new();
        store
            .create_issue(&make_issue("bd-lb1", "Labeled"), "user")
            .unwrap();

        store.add_label("bd-lb1", "bug", "user").unwrap();
        store.add_label("bd-lb1", "urgent", "user").unwrap();

        let labels = store.get_labels("bd-lb1");
        assert_eq!(labels.len(), 2);
        assert!(labels.contains(&"bug"));
        assert!(labels.contains(&"urgent"));

        store.remove_label("bd-lb1", "bug", "user").unwrap();
        let labels = store.get_labels("bd-lb1");
        assert_eq!(labels.len(), 1);
        assert!(labels.contains(&"urgent"));
    }

    #[test]
    fn test_comments() {
        let mut store = InMemoryStore::new();
        store
            .create_issue(&make_issue("bd-cm1", "Commented"), "user")
            .unwrap();

        let c1 = store
            .add_comment("bd-cm1", "alice", "First comment")
            .unwrap();
        let c2 = store
            .add_comment("bd-cm1", "bob", "Second comment")
            .unwrap();

        assert_eq!(c1.body, "First comment");
        assert_eq!(c2.body, "Second comment");
        assert_ne!(c1.id, c2.id);

        let comments = store.get_comments("bd-cm1");
        assert_eq!(comments.len(), 2);
    }

    #[test]
    fn test_ready_issues() {
        let mut store = InMemoryStore::new();
        store
            .create_issue(&make_issue("bd-r1", "Ready"), "user")
            .unwrap();
        store
            .create_issue(&make_issue("bd-r2", "Blocker"), "user")
            .unwrap();
        store
            .create_issue(&make_issue("bd-r3", "Blocked"), "user")
            .unwrap();

        store
            .add_dependency("bd-r3", "bd-r2", DependencyType::Blocks, "user", None)
            .unwrap();

        let ready = store.get_ready_issues(&ReadyFilters::default(), ReadySortPolicy::default());
        let ready_ids: Vec<&str> = ready.iter().map(|i| i.id.as_str()).collect();
        assert!(ready_ids.contains(&"bd-r1"));
        assert!(ready_ids.contains(&"bd-r2"));
        assert!(!ready_ids.contains(&"bd-r3"));
    }

    #[test]
    fn test_resolve_id() {
        let mut store = InMemoryStore::new();
        store
            .create_issue(&make_issue("bd-abc123", "Test"), "user")
            .unwrap();

        // Exact
        assert_eq!(store.resolve_id("bd-abc123").unwrap(), "bd-abc123");
        // Prefix-normalized
        assert_eq!(store.resolve_id("abc123").unwrap(), "bd-abc123");
        // Case insensitive
        assert_eq!(store.resolve_id("BD-ABC123").unwrap(), "bd-abc123");
        // Not found
        assert!(store.resolve_id("zzz").is_err());
    }

    #[test]
    fn test_dirty_tracking() {
        let mut store = InMemoryStore::new();
        assert!(!store.is_dirty());

        store
            .create_issue(&make_issue("bd-dt1", "Dirty"), "user")
            .unwrap();
        assert!(store.is_dirty());
        assert_eq!(store.dirty_count(), 1);

        store.clear_dirty();
        assert!(!store.is_dirty());
    }

    #[test]
    fn test_events_recorded() {
        let mut store = InMemoryStore::new();
        store
            .create_issue(&make_issue("bd-ev1", "Event test"), "user")
            .unwrap();

        let events = store.get_events("bd-ev1");
        assert!(!events.is_empty());
        assert_eq!(events[0].event_type, EventType::Created);
    }

    #[test]
    fn test_roundtrip_save_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("issues.jsonl");

        let mut store = InMemoryStore::new();
        store
            .create_issue(&make_issue("bd-rt1", "Roundtrip"), "user")
            .unwrap();
        store.add_label("bd-rt1", "test", "user").unwrap();
        store.add_comment("bd-rt1", "user", "Hello").unwrap();

        store.save_to(&path).unwrap();

        let loaded = InMemoryStore::open(&path).unwrap();
        assert_eq!(loaded.get_issue("bd-rt1").unwrap().title, "Roundtrip");
        assert_eq!(loaded.get_labels("bd-rt1"), vec!["test"]);
        assert_eq!(loaded.get_comments("bd-rt1").len(), 1);
    }

    #[test]
    fn test_list_with_label_filter() {
        let mut store = InMemoryStore::new();
        store
            .create_issue(&make_issue("bd-lf1", "Has label"), "user")
            .unwrap();
        store
            .create_issue(&make_issue("bd-lf2", "No label"), "user")
            .unwrap();
        store.add_label("bd-lf1", "important", "user").unwrap();

        let filtered = store.list_issues(&ListFilters {
            labels: Some(vec!["important".to_string()]),
            ..Default::default()
        });
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "bd-lf1");
    }

    #[test]
    fn test_get_unique_labels_with_counts() {
        let mut store = InMemoryStore::new();
        store
            .create_issue(&make_issue("bd-ulc1", "A"), "user")
            .unwrap();
        store
            .create_issue(&make_issue("bd-ulc2", "B"), "user")
            .unwrap();
        store.add_label("bd-ulc1", "bug", "user").unwrap();
        store.add_label("bd-ulc1", "urgent", "user").unwrap();
        store.add_label("bd-ulc2", "bug", "user").unwrap();

        let counts = store.get_unique_labels_with_counts();
        assert_eq!(counts.len(), 2);
        // "bug" should have count 2 and be first (sorted by count desc)
        assert_eq!(counts[0].0, "bug");
        assert_eq!(counts[0].1, 2);
        assert_eq!(counts[1].0, "urgent");
        assert_eq!(counts[1].1, 1);
    }

    #[test]
    fn test_list_types_filter() {
        let mut store = InMemoryStore::new();
        let mut bug = make_issue("bd-tf1", "A bug");
        bug.issue_type = IssueType::Bug;
        store.create_issue(&bug, "user").unwrap();

        let mut feat = make_issue("bd-tf2", "A feature");
        feat.issue_type = IssueType::Feature;
        store.create_issue(&feat, "user").unwrap();

        let bugs = store.list_issues(&ListFilters {
            types: Some(vec![IssueType::Bug]),
            ..Default::default()
        });
        assert_eq!(bugs.len(), 1);
        assert_eq!(bugs[0].id, "bd-tf1");
    }

    #[test]
    fn test_config() {
        let mut store = InMemoryStore::new();
        assert!(store.get_config("key").is_none());

        store.set_config("key", "value");
        assert_eq!(store.get_config("key"), Some("value"));
    }

    #[test]
    fn test_close_sets_closed_at() {
        let mut store = InMemoryStore::new();
        store
            .create_issue(&make_issue("bd-cls1", "Close me"), "user")
            .unwrap();

        let update = IssueUpdate {
            status: Some(Status::Closed),
            ..Default::default()
        };
        let updated = store.update_issue("bd-cls1", &update, "user").unwrap();
        assert!(updated.closed_at.is_some());
    }

    #[test]
    fn test_reopen_clears_closed_at() {
        let mut store = InMemoryStore::new();
        store
            .create_issue(&make_issue("bd-reo1", "Reopen me"), "user")
            .unwrap();

        store
            .update_issue(
                "bd-reo1",
                &IssueUpdate {
                    status: Some(Status::Closed),
                    ..Default::default()
                },
                "user",
            )
            .unwrap();

        let reopened = store
            .update_issue(
                "bd-reo1",
                &IssueUpdate {
                    status: Some(Status::Open),
                    ..Default::default()
                },
                "user",
            )
            .unwrap();
        assert!(reopened.closed_at.is_none());
    }

    #[test]
    fn test_get_blocked_issues() {
        let mut store = InMemoryStore::new();
        store
            .create_issue(&make_issue("bd-gbl1", "Blocker"), "user")
            .unwrap();
        store
            .create_issue(&make_issue("bd-gbl2", "Blocked"), "user")
            .unwrap();
        store
            .create_issue(&make_issue("bd-gbl3", "Free"), "user")
            .unwrap();

        store
            .add_dependency("bd-gbl2", "bd-gbl1", DependencyType::Blocks, "user", None)
            .unwrap();

        let blocked = store.get_blocked_issues();
        assert_eq!(blocked.len(), 1);
        assert_eq!(blocked[0].id, "bd-gbl2");
    }

    #[test]
    fn test_remove_dependency() {
        let mut store = InMemoryStore::new();
        store
            .create_issue(&make_issue("bd-rd1", "A"), "user")
            .unwrap();
        store
            .create_issue(&make_issue("bd-rd2", "B"), "user")
            .unwrap();

        store
            .add_dependency("bd-rd1", "bd-rd2", DependencyType::Blocks, "user", None)
            .unwrap();
        assert!(store.is_blocked("bd-rd1"));

        store.remove_dependency("bd-rd1", "bd-rd2", "user").unwrap();
        assert!(!store.is_blocked("bd-rd1"));
    }

    #[test]
    fn test_non_blocking_dep_doesnt_block() {
        let mut store = InMemoryStore::new();
        store
            .create_issue(&make_issue("bd-nb1", "A"), "user")
            .unwrap();
        store
            .create_issue(&make_issue("bd-nb2", "B"), "user")
            .unwrap();

        store
            .add_dependency("bd-nb1", "bd-nb2", DependencyType::Related, "user", None)
            .unwrap();

        assert!(!store.is_blocked("bd-nb1"));
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_load_project_jsonl() {
        // Load the project's own .beads/issues.jsonl if it exists
        let jsonl_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.beads/issues.jsonl");
        if !jsonl_path.exists() {
            return; // Skip if not present
        }

        let store = InMemoryStore::open(&jsonl_path).unwrap();
        assert!(!store.is_empty(), "Should have loaded issues");

        // --- Verify specific content from the first issue ---
        // We know from the file that the first issue has id "beads_rust-07b"
        let first = store.get_issue("beads_rust-07b");
        assert!(
            first.is_ok(),
            "Expected issue beads_rust-07b to exist, got: {:?}",
            first.err()
        );
        let first = first.unwrap();
        assert_eq!(first.title, "3-Way Merge Algorithm Implementation");
        assert_eq!(first.status, Status::Closed);
        assert_eq!(first.priority, crate::model::Priority::HIGH);
        assert_eq!(first.issue_type, crate::model::IssueType::Feature);
        assert_eq!(first.assignee.as_deref(), Some("GraySparrow"));
        assert!(
            first.closed_at.is_some(),
            "Closed issue should have closed_at"
        );

        // --- Verify query operations work on real data ---
        let all = store.list_issues(&ListFilters {
            include_closed: true,
            include_deferred: true,
            include_templates: true,
            ..Default::default()
        });
        assert_eq!(all.len(), store.len());

        let open_only = store.list_issues(&ListFilters::default());
        assert!(
            open_only.len() <= all.len(),
            "Open issues ({}) should be <= all issues ({})",
            open_only.len(),
            all.len()
        );

        // Check that closed issues are excluded by default
        for issue in &open_only {
            assert!(
                !issue.status.is_terminal(),
                "Default list should exclude terminal issues, found {} with status {}",
                issue.id,
                issue.status
            );
        }

        // --- Check labels were loaded ---
        let all_labels = store.get_unique_labels_with_counts();
        // The project has labels — verify structure
        for (label, count) in &all_labels {
            assert!(!label.is_empty(), "Label should not be empty");
            assert!(*count > 0, "Label count should be positive");
        }

        // --- Check dependencies were loaded ---
        let all_deps = store.get_all_dependency_records();
        // Verify dependency records reference existing issues
        for dep in &all_deps {
            assert!(
                store.id_exists(&dep.issue_id),
                "Dependency source {} should exist in store",
                dep.issue_id
            );
            // depends_on_id might reference a deleted issue, so just check it's non-empty
            assert!(
                !dep.depends_on_id.is_empty(),
                "Dependency target should not be empty"
            );
        }

        // --- Roundtrip: save, reload, compare every issue ---
        let dir = tempfile::tempdir().unwrap();
        let tmp_path = dir.path().join("roundtrip.jsonl");
        store.save_to(&tmp_path).unwrap();

        let reloaded = InMemoryStore::open(&tmp_path).unwrap();
        assert_eq!(
            reloaded.len(),
            store.len(),
            "Reloaded store should have same issue count"
        );

        // Compare every issue field by field
        for id in store.get_all_ids() {
            let original = store.get_issue(&id).unwrap();
            let roundtripped = reloaded.get_issue(&id).unwrap();

            assert_eq!(original.id, roundtripped.id, "ID mismatch for {id}");
            assert_eq!(
                original.title, roundtripped.title,
                "Title mismatch for {id}"
            );
            assert_eq!(
                original.description, roundtripped.description,
                "Description mismatch for {id}"
            );
            assert_eq!(
                original.status, roundtripped.status,
                "Status mismatch for {id}"
            );
            assert_eq!(
                original.priority, roundtripped.priority,
                "Priority mismatch for {id}"
            );
            assert_eq!(
                original.issue_type, roundtripped.issue_type,
                "Type mismatch for {id}"
            );
            assert_eq!(
                original.assignee, roundtripped.assignee,
                "Assignee mismatch for {id}"
            );

            // Check labels survived
            let orig_labels = store.get_labels(&id);
            let rt_labels = reloaded.get_labels(&id);
            assert_eq!(orig_labels, rt_labels, "Labels mismatch for {id}");

            // Check comments survived
            let orig_comments = store.get_comments(&id);
            let rt_comments = reloaded.get_comments(&id);
            assert_eq!(
                orig_comments.len(),
                rt_comments.len(),
                "Comment count mismatch for {id}"
            );
        }

        // Compare dependency counts
        assert_eq!(
            store.get_all_dependency_records().len(),
            reloaded.get_all_dependency_records().len(),
            "Dependency count mismatch after roundtrip"
        );

        // --- Verify the inferred prefix ---
        assert_eq!(store.prefix(), "beads_rust");
    }
}
