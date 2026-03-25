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

    // Deduplicate dependencies: the same dep may be embedded in both the
    // source and target issue lines.
    all_dependencies.sort_by(|a, b| {
        (&a.issue_id, &a.depends_on_id).cmp(&(&b.issue_id, &b.depends_on_id))
    });
    all_dependencies.dedup_by(|a, b| a.issue_id == b.issue_id && a.depends_on_id == b.depends_on_id);

    Ok(LoadedData {
        issues,
        labels: all_labels,
        dependencies: all_dependencies,
        comments: all_comments,
    })
}

/// Checks that a JSON string parses back as an `Issue` (round-trip parse check only).
/// Does not enforce domain invariants (e.g. title non-empty).
fn check_issue_json_roundtrip(json: &str, line: usize) -> Result<()> {
    serde_json::from_str::<Issue>(json).map_err(|e| BeadsError::JsonlWriteRoundtripFailed {
        line,
        reason: e.to_string(),
    })?;
    Ok(())
}

/// Save issues to a JSONL file with atomic write.
///
/// Each issue is serialized with its labels, dependencies, and comments
/// re-embedded. Each line is checked by parsing back to an `Issue` (round-trip
/// parse only; no domain validation). If the round-trip check fails, no file is
/// written. Uses write-to-temp + rename for atomicity.
///
/// # Errors
///
/// * [`BeadsError::Json`] if serialization fails
/// * [`BeadsError::JsonlWriteRoundtripFailed`] if a serialized line does not parse back as `Issue`
/// * [`BeadsError::Io`] if the file cannot be written
pub fn save(
    path: &Path,
    issues: &[Issue],
    labels: &[(String, Vec<String>)],
    dependencies: &[Dependency],
    comments: &[(String, Vec<Comment>)],
) -> Result<()> {
    use std::collections::HashMap;

    // Build lookup maps for reassembly
    let label_map: HashMap<&str, &Vec<String>> =
        labels.iter().map(|(id, l)| (id.as_str(), l)).collect();
    let comment_map: HashMap<&str, &Vec<Comment>> =
        comments.iter().map(|(id, c)| (id.as_str(), c)).collect();

    // Group dependencies by both issue_id and depends_on_id so that each
    // issue's JSONL line carries every dependency it participates in.
    let mut dep_map: HashMap<&str, Vec<&Dependency>> = HashMap::new();
    for dep in dependencies {
        dep_map.entry(dep.issue_id.as_str()).or_default().push(dep);
        if dep.depends_on_id != dep.issue_id {
            dep_map.entry(dep.depends_on_id.as_str()).or_default().push(dep);
        }
    }

    // Serialize and round-trip check every line before touching the filesystem.
    let mut lines = Vec::with_capacity(issues.len());
    for issue in issues {
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
        lines.push(json);
    }

    validate_then_write(path, &lines)
}

/// Round-trip parse check on every line, then writes to path. No file I/O until all pass.
fn validate_then_write(path: &Path, lines: &[String]) -> Result<()> {
    for (idx, json) in lines.iter().enumerate() {
        check_issue_json_roundtrip(json, idx + 1)?;
    }
    write_validated_lines(path, lines)
}

/// Writes pre-checked JSONL lines to path (temp file + atomic rename).
fn write_validated_lines(path: &Path, lines: &[String]) -> Result<()> {
    use std::io::Write;

    let tmp_path = path.with_extension("tmp");
    let mut file = fs::File::create(&tmp_path)?;
    for json in lines {
        writeln!(file, "{json}")?;
    }
    file.flush()?;
    drop(file);
    fs::rename(&tmp_path, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Comment, Dependency, DependencyType, Priority, Status};
    use chrono::Utc;

    fn minimal_issue_json() -> String {
        let now = Utc::now();
        let issue = Issue {
            id: "bd-abc".to_string(),
            title: "Test".to_string(),
            created_at: now,
            updated_at: now,
            ..Default::default()
        };
        serde_json::to_string(&issue).unwrap()
    }

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

    // --- check_issue_json_roundtrip (round-trip parse) tests ---

    #[test]
    fn test_check_issue_json_roundtrip_accepts_valid_issue() {
        let json = minimal_issue_json();
        check_issue_json_roundtrip(&json, 1).unwrap();
    }

    #[test]
    fn test_check_issue_json_roundtrip_rejects_truncated_json() {
        let json = r#"{"id":"x","title":"x"#;
        let err = check_issue_json_roundtrip(json, 1).unwrap_err();
        match &err {
            BeadsError::JsonlWriteRoundtripFailed { line, reason } => {
                assert_eq!(*line, 1);
                assert!(!reason.is_empty());
            }
            _ => panic!("expected JsonlWriteValidation, got {err:?}"),
        }
    }

    #[test]
    fn test_check_issue_json_roundtrip_rejects_empty_string() {
        let err = check_issue_json_roundtrip("", 1).unwrap_err();
        match &err {
            BeadsError::JsonlWriteRoundtripFailed { line, reason } => {
                assert_eq!(*line, 1);
                assert!(!reason.is_empty());
            }
            _ => panic!("expected JsonlWriteValidation, got {err:?}"),
        }
    }

    #[test]
    fn test_check_issue_json_roundtrip_rejects_invalid_syntax() {
        let err = check_issue_json_roundtrip("{]", 1).unwrap_err();
        match &err {
            BeadsError::JsonlWriteRoundtripFailed { .. } => {}
            _ => panic!("expected JsonlWriteValidation, got {err:?}"),
        }
    }

    #[test]
    fn test_check_issue_json_roundtrip_rejects_missing_required_field() {
        // Valid JSON but missing required Issue fields (e.g. created_at).
        let json = r#"{"id":"x","title":"x"}"#;
        let err = check_issue_json_roundtrip(json, 1).unwrap_err();
        match &err {
            BeadsError::JsonlWriteRoundtripFailed { line, reason } => {
                assert_eq!(*line, 1);
                assert!(!reason.is_empty());
            }
            _ => panic!("expected JsonlWriteValidation, got {err:?}"),
        }
    }

    #[test]
    fn test_check_issue_json_roundtrip_reports_line_number() {
        let bad = r#"{"id":"x","title":"x"#;
        let err = check_issue_json_roundtrip(bad, 42).unwrap_err();
        match &err {
            BeadsError::JsonlWriteRoundtripFailed { line, .. } => assert_eq!(*line, 42),
            _ => panic!("expected JsonlWriteValidation, got {err:?}"),
        }
    }

    // --- validate_then_write: no file when validation fails ---

    #[test]
    fn test_validate_then_write_rejects_invalid_line_and_does_not_create_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.jsonl");
        let valid = minimal_issue_json();
        let lines = vec![valid, r#"{"id":"y","title"#.to_string()];

        let err = validate_then_write(&path, &lines).unwrap_err();
        match &err {
            BeadsError::JsonlWriteRoundtripFailed { line, .. } => assert_eq!(*line, 2),
            _ => panic!("expected JsonlWriteValidation, got {err:?}"),
        }
        assert!(
            !path.exists(),
            "target file must not be created on validation failure"
        );
    }

    #[test]
    fn test_validate_then_write_with_valid_lines_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.jsonl");
        let valid = minimal_issue_json();
        let lines = vec![valid];

        validate_then_write(&path, &lines).unwrap();
        assert!(path.exists());
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.issues.len(), 1);
        assert_eq!(loaded.issues[0].id, "bd-abc");
    }

    #[test]
    fn test_validate_then_write_first_line_invalid_no_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.jsonl");
        let lines = vec!["not valid json".to_string()];

        let err = validate_then_write(&path, &lines).unwrap_err();
        match &err {
            BeadsError::JsonlWriteRoundtripFailed { line, .. } => assert_eq!(*line, 1),
            _ => panic!("expected JsonlWriteValidation, got {err:?}"),
        }
        assert!(!path.exists());
    }

    #[test]
    fn test_save_multiple_issues_validates_all_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("multi.jsonl");
        let now = Utc::now();
        let issues = vec![
            Issue {
                id: "bd-a".to_string(),
                title: "First".to_string(),
                created_at: now,
                updated_at: now,
                ..Default::default()
            },
            Issue {
                id: "bd-b".to_string(),
                title: "Second".to_string(),
                created_at: now,
                updated_at: now,
                ..Default::default()
            },
            Issue {
                id: "bd-c".to_string(),
                title: "Third".to_string(),
                created_at: now,
                updated_at: now,
                ..Default::default()
            },
        ];
        let labels: Vec<(String, Vec<String>)> = vec![];
        let deps: Vec<Dependency> = vec![];
        let comments: Vec<(String, Vec<Comment>)> = vec![];

        save(&path, &issues, &labels, &deps, &comments).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.issues.len(), 3);
        assert_eq!(loaded.issues[0].id, "bd-a");
        assert_eq!(loaded.issues[1].id, "bd-b");
        assert_eq!(loaded.issues[2].id, "bd-c");
    }

    #[test]
    fn test_roundtrip_preserves_target_dependencies() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("deps.jsonl");
        let now = Utc::now();

        let issue_a = Issue {
            id: "bd-abc".to_string(),
            title: "Issue A".to_string(),
            created_at: now,
            updated_at: now,
            ..Default::default()
        };
        let issue_b = Issue {
            id: "bd-def".to_string(),
            title: "Issue B".to_string(),
            created_at: now,
            updated_at: now,
            ..Default::default()
        };

        let dep = Dependency {
            issue_id: "bd-abc".to_string(),
            depends_on_id: "bd-def".to_string(),
            dep_type: DependencyType::Blocks,
            created_at: now,
            created_by: None,
            metadata: None,
            thread_id: None,
        };

        save(
            &path,
            &[issue_a, issue_b],
            &[],
            &[dep.clone()],
            &[],
        )
        .unwrap();

        // Verify that both issue lines contain the dependency
        let raw = std::fs::read_to_string(&path).unwrap();
        for line in raw.lines() {
            let issue: Issue = serde_json::from_str(line).unwrap();
            assert_eq!(
                issue.dependencies.len(),
                1,
                "Issue {} should have the dependency embedded",
                issue.id
            );
        }

        // Verify load deduplicates correctly
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.dependencies.len(), 1);
        assert_eq!(loaded.dependencies[0].issue_id, "bd-abc");
        assert_eq!(loaded.dependencies[0].depends_on_id, "bd-def");
    }
}
