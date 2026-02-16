//! JSONL file I/O for beads issues.
//!
//! Each line in the JSONL file is a complete Issue with embedded
//! labels, dependencies, and comments.

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;

use crate::error::{BeadsError, Result};
use crate::model::{Comment, Dependency, Issue};

/// Loaded issue data with relations extracted into separate collections.
pub struct LoadedData {
    pub issues: Vec<Issue>,
    pub labels: Vec<(String, Vec<String>)>,
    pub dependencies: Vec<Dependency>,
    pub comments: Vec<(String, Vec<Comment>)>,
}

/// Load issues from a JSONL file.
///
/// Each line is parsed as a complete `Issue`. Embedded labels,
/// dependencies, and comments are extracted into separate collections
/// while the bare issue keeps empty relation vectors.
///
/// # Errors
///
/// Returns `Io` if the file cannot be read, or `JsonlParse` if any line is invalid.
pub fn load(path: &Path) -> Result<LoadedData> {
    let file = fs::File::open(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            BeadsError::FileNotFound(path.to_path_buf())
        } else {
            BeadsError::Io(e)
        }
    })?;
    let reader = BufReader::new(file);

    let mut issues = Vec::new();
    let mut all_labels = Vec::new();
    let mut all_dependencies = Vec::new();
    let mut all_comments = Vec::new();

    for (line_num, line) in reader.lines().enumerate() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let mut issue: Issue =
            serde_json::from_str(trimmed).map_err(|e| BeadsError::JsonlParse {
                line: line_num + 1,
                reason: e.to_string(),
            })?;

        // Extract embedded relations
        if !issue.labels.is_empty() {
            all_labels.push((issue.id.clone(), std::mem::take(&mut issue.labels)));
        }
        if !issue.dependencies.is_empty() {
            all_dependencies.extend(std::mem::take(&mut issue.dependencies));
        }
        if !issue.comments.is_empty() {
            all_comments.push((issue.id.clone(), std::mem::take(&mut issue.comments)));
        }

        issues.push(issue);
    }

    Ok(LoadedData {
        issues,
        labels: all_labels,
        dependencies: all_dependencies,
        comments: all_comments,
    })
}

/// Save issues to a JSONL file with atomic write.
///
/// Each issue is serialized with its labels, dependencies, and comments
/// re-embedded. Uses write-to-temp + rename for atomicity.
///
/// # Errors
///
/// Returns `Io` if the file cannot be written.
pub fn save(
    path: &Path,
    issues: &[Issue],
    labels: &[(String, Vec<String>)],
    dependencies: &[Dependency],
    comments: &[(String, Vec<Comment>)],
) -> Result<()> {
    use std::collections::HashMap;
    use std::io::Write;

    // Build lookup maps for reassembly
    let label_map: HashMap<&str, &Vec<String>> =
        labels.iter().map(|(id, l)| (id.as_str(), l)).collect();
    let comment_map: HashMap<&str, &Vec<Comment>> =
        comments.iter().map(|(id, c)| (id.as_str(), c)).collect();

    // Group dependencies by issue_id
    let mut dep_map: HashMap<&str, Vec<&Dependency>> = HashMap::new();
    for dep in dependencies {
        dep_map.entry(dep.issue_id.as_str()).or_default().push(dep);
    }

    // Write to temp file
    let tmp_path = path.with_extension("jsonl.tmp");
    let mut file = fs::File::create(&tmp_path)?;

    for issue in issues {
        // Reassemble issue with embedded relations
        let mut full_issue = issue.clone();
        if let Some(labels) = label_map.get(issue.id.as_str()) {
            full_issue.labels.clone_from(labels);
        }
        if let Some(deps) = dep_map.get(issue.id.as_str()) {
            full_issue.dependencies = deps.iter().map(|d| (*d).clone()).collect();
        }
        if let Some(comms) = comment_map.get(issue.id.as_str()) {
            full_issue.comments.clone_from(comms);
        }

        let json = serde_json::to_string(&full_issue)?;
        writeln!(file, "{json}")?;
    }

    file.flush()?;
    drop(file);

    // Atomic rename
    fs::rename(&tmp_path, path)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Priority, Status};
    use chrono::Utc;

    #[test]
    fn test_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("issues.jsonl");

        let now = Utc::now();
        let issues = vec![Issue {
            id: "bd-abc".to_string(),
            title: "Test issue".to_string(),
            status: Status::Open,
            priority: Priority::MEDIUM,
            created_at: now,
            updated_at: now,
            ..Default::default()
        }];
        let labels = vec![("bd-abc".to_string(), vec!["bug".to_string()])];
        let deps = vec![];
        let comments = vec![];

        save(&path, &issues, &labels, &deps, &comments).unwrap();

        let loaded = load(&path).unwrap();
        assert_eq!(loaded.issues.len(), 1);
        assert_eq!(loaded.issues[0].id, "bd-abc");
        assert_eq!(loaded.issues[0].title, "Test issue");
        assert_eq!(loaded.labels.len(), 1);
        assert_eq!(loaded.labels[0].1, vec!["bug".to_string()]);
    }

    #[test]
    fn test_load_missing_file() {
        let result = load(Path::new("/nonexistent/issues.jsonl"));
        assert!(result.is_err());
    }

    #[test]
    fn test_load_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.jsonl");
        fs::write(&path, "").unwrap();

        let loaded = load(&path).unwrap();
        assert!(loaded.issues.is_empty());
    }

    #[test]
    fn test_load_skips_blank_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("blanks.jsonl");
        let now = Utc::now();
        let issue = Issue {
            id: "bd-123".to_string(),
            title: "Test".to_string(),
            created_at: now,
            updated_at: now,
            ..Default::default()
        };
        let json = serde_json::to_string(&issue).unwrap();
        fs::write(&path, format!("\n{json}\n\n")).unwrap();

        let loaded = load(&path).unwrap();
        assert_eq!(loaded.issues.len(), 1);
    }
}
