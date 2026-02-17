//! ID generation and content hashing utilities.

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

use crate::model::{Issue, IssueType, Priority, Status};

// ============================================================================
// ID Generation
// ============================================================================

/// Generate a unique issue ID with the given prefix.
///
/// Uses SHA256 hashing with base36 encoding.
/// The `exists` closure checks for collisions.
pub fn generate_id<F>(
    prefix: &str,
    title: &str,
    description: Option<&str>,
    creator: Option<&str>,
    created_at: DateTime<Utc>,
    issue_count: usize,
    exists: F,
) -> String
where
    F: Fn(&str) -> bool,
{
    let mut length = optimal_hash_length(issue_count);

    loop {
        for nonce in 0..10 {
            let seed = generate_id_seed(title, description, creator, created_at, nonce);
            let hash_str = compute_id_hash(&seed, length);
            let id = format!("{prefix}-{hash_str}");
            if !exists(&id) {
                return id;
            }
        }

        if length < 8 {
            length += 1;
        } else {
            // Fallback: use longer hash with increasing nonces
            let mut nonce = 0u32;
            loop {
                let seed = generate_id_seed(title, description, creator, created_at, nonce);
                let hash_str = compute_id_hash(&seed, 12);
                let id = format!("{prefix}-{hash_str}");
                if !exists(&id) {
                    return id;
                }
                nonce += 1;
                if nonce > 1000 {
                    return format!("{prefix}-{hash_str}{nonce}");
                }
            }
        }
    }
}

/// Compute the optimal hash length for a given issue count.
#[must_use]
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
fn optimal_hash_length(issue_count: usize) -> usize {
    let n = issue_count as f64;
    let max_prob = 0.25;

    for (len, exp) in [(3_usize, 3_i32), (4, 4), (5, 5), (6, 6), (7, 7), (8, 8)] {
        let space = 36_f64.powi(exp);
        let prob = 1.0 - (-n * n / (2.0 * space)).exp();
        if prob < max_prob {
            return len;
        }
    }
    8
}

fn generate_id_seed(
    title: &str,
    description: Option<&str>,
    creator: Option<&str>,
    created_at: DateTime<Utc>,
    nonce: u32,
) -> String {
    format!(
        "{}|{}|{}|{}|{}",
        title,
        description.unwrap_or(""),
        creator.unwrap_or(""),
        created_at.timestamp_nanos_opt().unwrap_or(0),
        nonce
    )
}

fn compute_id_hash(input: &str, length: usize) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize();

    let mut num = 0u64;
    for &byte in result.iter().take(8) {
        num = (num << 8) | u64::from(byte);
    }

    let mut encoded = base36_encode(num);
    if encoded.len() < length {
        encoded = format!("{encoded:0>length$}");
    }
    encoded.chars().take(length).collect()
}

fn base36_encode(mut num: u64) -> String {
    const ALPHABET: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if num == 0 {
        return "0".to_string();
    }
    let mut chars = Vec::new();
    while num > 0 {
        chars.push(ALPHABET[(num % 36) as usize] as char);
        num /= 36;
    }
    chars.into_iter().rev().collect()
}

// ============================================================================
// Content Hashing
// ============================================================================

/// Compute SHA256 content hash for an issue.
///
/// Fields included (stable order with null separators):
/// title, description, design, acceptance_criteria, notes,
/// status, priority, issue_type, assignee, owner, created_by,
/// external_ref, source_system, pinned, is_template.
///
/// Fields excluded: id, timestamps, labels, dependencies, comments, tombstone fields.
#[must_use]
pub fn content_hash(issue: &Issue) -> String {
    content_hash_from_parts(
        &issue.title,
        issue.description.as_deref(),
        issue.design.as_deref(),
        issue.acceptance_criteria.as_deref(),
        issue.notes.as_deref(),
        &issue.status,
        &issue.priority,
        &issue.issue_type,
        issue.assignee.as_deref(),
        issue.owner.as_deref(),
        issue.created_by.as_deref(),
        issue.external_ref.as_deref(),
        issue.source_system.as_deref(),
        issue.pinned,
        issue.is_template,
    )
}

/// Create a content hash from raw components.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn content_hash_from_parts(
    title: &str,
    description: Option<&str>,
    design: Option<&str>,
    acceptance_criteria: Option<&str>,
    notes: Option<&str>,
    status: &Status,
    priority: &Priority,
    issue_type: &IssueType,
    assignee: Option<&str>,
    owner: Option<&str>,
    created_by: Option<&str>,
    external_ref: Option<&str>,
    source_system: Option<&str>,
    pinned: bool,
    is_template: bool,
) -> String {
    let mut hasher = Sha256::new();

    let mut hash_field = |value: &str| {
        if value.contains('\0') {
            hasher.update(value.replace('\0', " ").as_bytes());
        } else {
            hasher.update(value.as_bytes());
        }
        hasher.update(b"\x00");
    };

    hash_field(title);
    hash_field(description.unwrap_or(""));
    hash_field(design.unwrap_or(""));
    hash_field(acceptance_criteria.unwrap_or(""));
    hash_field(notes.unwrap_or(""));
    hash_field(status.as_str());
    hash_field(&format!("P{}", priority.0));
    hash_field(issue_type.as_str());
    hash_field(assignee.unwrap_or(""));
    hash_field(owner.unwrap_or(""));
    hash_field(created_by.unwrap_or(""));
    hash_field(external_ref.unwrap_or(""));
    hash_field(source_system.unwrap_or(""));
    hash_field(if pinned { "true" } else { "false" });
    hash_field(if is_template { "true" } else { "false" });

    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_hash_deterministic() {
        let issue = Issue {
            title: "Test".to_string(),
            ..Default::default()
        };
        let h1 = content_hash(&issue);
        let h2 = content_hash(&issue);
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64);
    }

    #[test]
    fn test_content_hash_changes_with_title() {
        let i1 = Issue {
            title: "A".to_string(),
            ..Default::default()
        };
        let i2 = Issue {
            title: "B".to_string(),
            ..Default::default()
        };
        assert_ne!(content_hash(&i1), content_hash(&i2));
    }

    #[test]
    fn test_content_hash_ignores_timestamps() {
        let i1 = Issue {
            title: "T".to_string(),
            ..Default::default()
        };
        let mut i2 = i1.clone();
        i2.created_at = Utc::now();
        i2.updated_at = Utc::now();
        assert_eq!(content_hash(&i1), content_hash(&i2));
    }

    #[test]
    fn test_generate_id_format() {
        let id = generate_id("bd", "Test", None, None, Utc::now(), 0, |_| false);
        assert!(id.starts_with("bd-"));
        assert!(id.len() >= 6);
    }

    #[test]
    fn test_generate_id_collision_handling() {
        let mut generated = std::collections::HashSet::new();
        let now = Utc::now();
        let id1 = generate_id("bd", "Test", None, None, now, 0, |id| {
            generated.contains(id)
        });
        generated.insert(id1.clone());
        let id2 = generate_id("bd", "Test", None, None, now, 0, |id| {
            generated.contains(id)
        });
        assert_ne!(id1, id2);
    }
}
