//! ID generation for issues.
//!
//! Implements classic bd ID format: `<prefix>-<hash>` where hash is
//! base36 lowercase (0-9, a-z) with adaptive length based on DB size.

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

/// Default ID generation configuration.
#[derive(Debug, Clone)]
pub struct IdConfig {
    /// Issue ID prefix (e.g., "bd", "`beads_rust`").
    pub prefix: String,
    /// Minimum hash length.
    pub min_hash_length: usize,
    /// Maximum hash length.
    pub max_hash_length: usize,
    /// Maximum collision probability before increasing length.
    pub max_collision_prob: f64,
}

impl Default for IdConfig {
    fn default() -> Self {
        Self {
            prefix: "bd".to_string(),
            min_hash_length: 3,
            max_hash_length: 8,
            max_collision_prob: 0.25,
        }
    }
}

impl IdConfig {
    /// Create a new ID config with the given prefix.
    #[must_use]
    pub fn with_prefix(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
            ..Default::default()
        }
    }
}

/// ID generator that produces unique issue IDs.
#[derive(Debug, Clone)]
pub struct IdGenerator {
    config: IdConfig,
}

impl IdGenerator {
    /// Create a new ID generator with the given config.
    #[must_use]
    pub const fn new(config: IdConfig) -> Self {
        Self { config }
    }

    /// Create a new ID generator with default config.
    #[must_use]
    pub fn with_defaults() -> Self {
        Self::new(IdConfig::default())
    }

    /// Get the configured prefix.
    #[must_use]
    pub fn prefix(&self) -> &str {
        &self.config.prefix
    }

    /// Compute the optimal hash length for a given issue count.
    ///
    /// Uses birthday problem approximation to estimate collision probability.
    #[must_use]
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap
    )]
    pub fn optimal_length(&self, issue_count: usize) -> usize {
        let n = issue_count as f64;
        let max_prob = self.config.max_collision_prob;

        for len in self.config.min_hash_length..=self.config.max_hash_length {
            // Base36 has 36^len possible values
            let space = 36_f64.powi(len as i32);
            // Birthday problem: P(collision) ≈ 1 - e^(-n²/2d)
            let prob = 1.0 - (-n * n / (2.0 * space)).exp();
            if prob < max_prob {
                return len;
            }
        }
        self.config.max_hash_length
    }

    /// Generate a candidate ID with the given parameters.
    #[must_use]
    pub fn generate_candidate(
        &self,
        title: &str,
        description: Option<&str>,
        creator: Option<&str>,
        created_at: DateTime<Utc>,
        nonce: u32,
        hash_length: usize,
    ) -> String {
        let seed = generate_id_seed(title, description, creator, created_at, nonce);
        let hash_str = compute_id_hash(&seed, hash_length);
        format!("{}-{hash_str}", self.config.prefix)
    }

    /// Generate an ID, checking for collisions with the provided checker.
    ///
    /// The checker function should return `true` if the ID already exists.
    pub fn generate<F>(
        &self,
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
        let mut length = self.optimal_length(issue_count);

        loop {
            // Try nonces 0..10 at this length
            for nonce in 0..10 {
                let id =
                    self.generate_candidate(title, description, creator, created_at, nonce, length);
                if !exists(&id) {
                    return id;
                }
            }

            // All nonces collided, increase length
            if length < self.config.max_hash_length {
                length += 1;
            } else {
                // Fallback: use full hash with extra entropy
                let seed = generate_id_seed(title, description, creator, created_at, 0);
                let hash_str = compute_id_hash(&seed, 12);
                return format!("{}-{hash_str}", self.config.prefix);
            }
        }
    }
}

/// Generate the seed string for ID generation.
///
/// Inputs: `title | description | creator | created_at (ns) | nonce`
#[must_use]
pub fn generate_id_seed(
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

/// Compute a base36 hash of the input string with a specific length.
///
/// Uses SHA256 to hash the input, then converts the first 8 bytes to a u64,
/// encodes as base36, and truncates to the requested length.
#[must_use]
pub fn compute_id_hash(input: &str, length: usize) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize();

    // Use first 8 bytes for a 64-bit integer
    let mut num = 0u64;
    for &byte in result.iter().take(8) {
        num = (num << 8) | u64::from(byte);
    }

    let encoded = base36_encode(num);

    // Pad with '0' if too short (unlikely for u64 but possible)
    let mut s = encoded;
    if s.len() < length {
        s = format!("{s:0>length$}");
    }

    // Take the first `length` chars
    s.chars().take(length).collect()
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
// Child ID Helpers
// ============================================================================

/// Generate child ID from parent.
///
/// Child IDs have format: `<parent>.<n>` where n is the child number.
#[must_use]
pub fn child_id(parent_id: &str, child_number: u32) -> String {
    format!("{parent_id}.{child_number}")
}

/// Check if an ID is a child ID (contains a dot after the hash).
#[must_use]
pub fn is_child_id(id: &str) -> bool {
    // Only check after the prefix-hash part
    id.find('-')
        .map_or_else(|| id.contains('.'), |pos| id[pos + 1..].contains('.'))
}

/// Get the depth of a hierarchical ID.
///
/// Top-level IDs have depth 0, first-level children have depth 1, etc.
#[must_use]
pub fn id_depth(id: &str) -> usize {
    // Count dots after the prefix-hash part
    id.find('-').map_or_else(
        || id.matches('.').count(),
        |pos| id[pos + 1..].matches('.').count(),
    )
}

/// Convenience function to generate an ID with default settings.
#[must_use]
pub fn generate_id(
    title: &str,
    description: Option<&str>,
    creator: Option<&str>,
    created_at: DateTime<Utc>,
) -> String {
    let generator = IdGenerator::with_defaults();
    generator.generate(title, description, creator, created_at, 0, |_| false)
}

// ============================================================================
// ID Parsing and Validation
// ============================================================================

use crate::error::{BeadsError, Result};

/// Parsed components of an issue ID.
///
/// Supports both root IDs (`bd-abc123`) and hierarchical IDs (`bd-abc123.1.2`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedId {
    /// The prefix (e.g., "bd").
    pub prefix: String,
    /// The hash portion (e.g., "abc123").
    pub hash: String,
    /// Child path segments if this is a hierarchical ID (e.g., `[1, 2]` for `.1.2`).
    pub child_path: Vec<u32>,
}

impl ParsedId {
    /// Returns true if this is a root (non-hierarchical) ID.
    #[must_use]
    pub fn is_root(&self) -> bool {
        self.child_path.is_empty()
    }

    /// Returns the depth in the hierarchy (0 for root).
    #[must_use]
    pub fn depth(&self) -> usize {
        self.child_path.len()
    }

    /// Get the parent ID if this is a child.
    ///
    /// Returns `None` for root IDs.
    #[must_use]
    pub fn parent(&self) -> Option<String> {
        if self.child_path.is_empty() {
            return None;
        }

        let mut parent_path = self.child_path.clone();
        parent_path.pop();

        if parent_path.is_empty() {
            Some(format!("{}-{}", self.prefix, self.hash))
        } else {
            let path_str = format_child_path(&parent_path);
            Some(format!("{}-{}{}", self.prefix, self.hash, path_str))
        }
    }

    /// Reconstruct the full ID string.
    #[must_use]
    pub fn to_id_string(&self) -> String {
        if self.child_path.is_empty() {
            format!("{}-{}", self.prefix, self.hash)
        } else {
            let path_str = format_child_path(&self.child_path);
            format!("{}-{}{}", self.prefix, self.hash, path_str)
        }
    }

    /// Check if this ID is a child (direct or indirect) of another.
    #[must_use]
    pub fn is_child_of(&self, potential_parent: &str) -> bool {
        let full_id = self.to_id_string();
        full_id.starts_with(potential_parent)
            && full_id.len() > potential_parent.len()
            && full_id.chars().nth(potential_parent.len()) == Some('.')
    }
}

fn format_child_path(path: &[u32]) -> String {
    let mut out = String::new();
    for segment in path {
        use std::fmt::Write;
        let _ = write!(out, ".{segment}");
    }
    out
}

/// Parse an issue ID into its components.
///
/// # Errors
///
/// Returns `InvalidId` if the ID format is invalid.
pub fn parse_id(id: &str) -> Result<ParsedId> {
    // Find the prefix-hash separator
    let Some(dash_pos) = id.find('-') else {
        return Err(BeadsError::InvalidId { id: id.to_string() });
    };

    let prefix = &id[..dash_pos];
    let remainder = &id[dash_pos + 1..];

    if prefix.is_empty() || remainder.is_empty() {
        return Err(BeadsError::InvalidId { id: id.to_string() });
    }

    // Split remainder by '.' to get hash and child path
    let parts: Vec<&str> = remainder.split('.').collect();
    let hash = parts[0].to_string();

    if hash.is_empty() {
        return Err(BeadsError::InvalidId { id: id.to_string() });
    }

    // Validate hash is base36 (lowercase alphanumeric)
    if !hash
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    {
        return Err(BeadsError::InvalidId { id: id.to_string() });
    }

    // Parse child path segments
    let mut child_path = Vec::new();
    for part in parts.iter().skip(1) {
        match part.parse::<u32>() {
            Ok(n) => child_path.push(n),
            Err(_) => return Err(BeadsError::InvalidId { id: id.to_string() }),
        }
    }

    Ok(ParsedId {
        prefix: prefix.to_string(),
        hash,
        child_path,
    })
}

/// Validate that an ID has the expected prefix.
///
/// # Arguments
///
/// * `id` - The ID to validate
/// * `expected_prefix` - The primary expected prefix
/// * `allowed_prefixes` - Additional allowed prefixes
///
/// # Errors
///
/// Returns `PrefixMismatch` if the prefix doesn't match expected or allowed.
pub fn validate_prefix(id: &str, expected_prefix: &str, allowed_prefixes: &[String]) -> Result<()> {
    let parsed = parse_id(id)?;

    if parsed.prefix == expected_prefix {
        return Ok(());
    }

    if allowed_prefixes.contains(&parsed.prefix) {
        return Ok(());
    }

    Err(BeadsError::PrefixMismatch {
        expected: expected_prefix.to_string(),
        found: parsed.prefix,
    })
}

/// Normalize an ID to consistent lowercase format.
#[must_use]
pub fn normalize_id(id: &str) -> String {
    id.to_lowercase()
}

/// Check if a string looks like a valid issue ID format.
#[must_use]
pub fn is_valid_id_format(id: &str) -> bool {
    parse_id(id).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base36_encode() {
        assert_eq!(base36_encode(0), "0");
        assert_eq!(base36_encode(10), "a");
        assert_eq!(base36_encode(35), "z");
        assert_eq!(base36_encode(36), "10");
    }

    #[test]
    fn test_compute_id_hash_length() {
        let input = "test input";
        let hash3 = compute_id_hash(input, 3);
        assert_eq!(hash3.len(), 3);

        let hash8 = compute_id_hash(input, 8);
        assert_eq!(hash8.len(), 8);
    }

    #[test]
    fn test_generate_id_seed() {
        let now = Utc::now();
        let seed = generate_id_seed("title", Some("desc"), Some("me"), now, 0);
        assert!(seed.contains("title"));
        assert!(seed.contains("desc"));
        assert!(seed.contains("me"));
        assert!(seed.ends_with("|0"));
    }

    #[test]
    fn test_parse_id_root() {
        let parsed = parse_id("bd-abc123").unwrap();
        assert_eq!(parsed.prefix, "bd");
        assert_eq!(parsed.hash, "abc123");
        assert!(parsed.child_path.is_empty());
        assert!(parsed.is_root());
        assert_eq!(parsed.depth(), 0);
    }

    #[test]
    fn test_parse_id_child() {
        let parsed = parse_id("bd-abc123.1").unwrap();
        assert_eq!(parsed.prefix, "bd");
        assert_eq!(parsed.hash, "abc123");
        assert_eq!(parsed.child_path, vec![1]);
        assert!(!parsed.is_root());
        assert_eq!(parsed.depth(), 1);
    }

    #[test]
    fn test_parse_id_grandchild() {
        let parsed = parse_id("bd-abc123.1.2").unwrap();
        assert_eq!(parsed.child_path, vec![1, 2]);
        assert_eq!(parsed.depth(), 2);
    }

    #[test]
    fn test_parse_id_invalid_no_dash() {
        assert!(parse_id("bdabc123").is_err());
    }

    #[test]
    fn test_parse_id_invalid_empty_hash() {
        assert!(parse_id("bd-").is_err());
    }

    #[test]
    fn test_parse_id_invalid_uppercase() {
        assert!(parse_id("bd-ABC123").is_err());
    }

    #[test]
    fn test_parsed_id_parent() {
        let child = parse_id("bd-abc123.1").unwrap();
        assert_eq!(child.parent(), Some("bd-abc123".to_string()));

        let grandchild = parse_id("bd-abc123.1.2").unwrap();
        assert_eq!(grandchild.parent(), Some("bd-abc123.1".to_string()));

        let root = parse_id("bd-abc123").unwrap();
        assert_eq!(root.parent(), None);
    }

    #[test]
    fn test_parsed_id_to_string() {
        let root = parse_id("bd-abc123").unwrap();
        assert_eq!(root.to_id_string(), "bd-abc123");

        let child = parse_id("bd-abc123.1.2").unwrap();
        assert_eq!(child.to_id_string(), "bd-abc123.1.2");
    }

    #[test]
    fn test_parsed_id_is_child_of() {
        let child = parse_id("bd-abc123.1").unwrap();
        assert!(child.is_child_of("bd-abc123"));
        assert!(!child.is_child_of("bd-xyz"));

        let grandchild = parse_id("bd-abc123.1.2").unwrap();
        assert!(grandchild.is_child_of("bd-abc123"));
        assert!(grandchild.is_child_of("bd-abc123.1"));
    }

    #[test]
    fn test_validate_prefix() {
        assert!(validate_prefix("bd-abc123", "bd", &[]).is_ok());
        assert!(validate_prefix("bd-abc123", "other", &["bd".to_string()]).is_ok());
        assert!(validate_prefix("bd-abc123", "other", &[]).is_err());
    }

    #[test]
    fn test_is_valid_id_format() {
        assert!(is_valid_id_format("bd-abc123"));
        assert!(is_valid_id_format("bd-abc123.1.2"));
        assert!(!is_valid_id_format("invalid"));
        assert!(!is_valid_id_format("bd-ABC")); // uppercase
    }

    #[test]
    fn test_id_generator_optimal_length() {
        let id_gen = IdGenerator::with_defaults();

        // Small DB should use minimum length
        assert_eq!(id_gen.optimal_length(0), 3);
        assert_eq!(id_gen.optimal_length(10), 3);

        // Large DB should need more characters
        let len_1000 = id_gen.optimal_length(1000);
        assert!(len_1000 >= 3);
        assert!(len_1000 <= 8);
    }

    #[test]
    fn test_id_generator_generate() {
        let id_gen = IdGenerator::with_defaults();
        let now = Utc::now();

        let id = id_gen.generate(
            "Test Issue",
            Some("Description"),
            Some("user"),
            now,
            0,
            |_| false,
        );

        assert!(id.starts_with("bd-"));
        assert!(is_valid_id_format(&id));
    }

    #[test]
    fn test_id_generator_collision_handling() {
        let id_gen = IdGenerator::with_defaults();
        let now = Utc::now();

        let mut generated = std::collections::HashSet::new();

        // Generate first ID
        let id1 = id_gen.generate("Test", None, None, now, 0, |id| generated.contains(id));
        generated.insert(id1.clone());

        // Generate second ID - should get different one due to collision check
        let id2 = id_gen.generate("Test", None, None, now, 0, |id| generated.contains(id));

        assert_ne!(id1, id2);
    }
}
