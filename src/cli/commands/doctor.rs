//! Doctor command implementation.

use crate::error::Result;
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
enum CheckStatus {
    Ok,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize)]
struct CheckResult {
    name: String,
    status: CheckStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
struct DoctorReport {
    ok: bool,
    checks: Vec<CheckResult>,
}

fn push_check(
    checks: &mut Vec<CheckResult>,
    name: &str,
    status: CheckStatus,
    message: Option<String>,
    details: Option<serde_json::Value>,
) {
    checks.push(CheckResult {
        name: name.to_string(),
        status,
        message,
        details,
    });
}

fn has_error(checks: &[CheckResult]) -> bool {
    checks
        .iter()
        .any(|check| matches!(check.status, CheckStatus::Error))
}

fn print_report(report: &DoctorReport, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string(&report)?);
        return Ok(());
    }

    println!("br doctor");
    for check in &report.checks {
        let label = match check.status {
            CheckStatus::Ok => "OK",
            CheckStatus::Warn => "WARN",
            CheckStatus::Error => "ERROR",
        };
        if let Some(message) = &check.message {
            println!("{label} {}: {}", check.name, message);
        } else {
            println!("{label} {}", check.name);
        }
    }
    Ok(())
}

fn collect_table_columns(conn: &Connection, table: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let mut columns = Vec::new();
    for row in rows {
        columns.push(row?);
    }
    Ok(columns)
}

fn required_schema_checks(conn: &Connection, checks: &mut Vec<CheckResult>) -> Result<()> {
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
    )?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut tables = Vec::new();
    for row in rows {
        tables.push(row?);
    }

    let required_tables = [
        "issues",
        "dependencies",
        "labels",
        "comments",
        "events",
        "config",
        "metadata",
        "dirty_issues",
        "export_hashes",
        "blocked_issues_cache",
        "child_counters",
    ];
    let missing_tables: Vec<&str> = required_tables
        .iter()
        .copied()
        .filter(|table| !tables.iter().any(|t| t == table))
        .collect();

    if missing_tables.is_empty() {
        push_check(
            checks,
            "schema.tables",
            CheckStatus::Ok,
            None,
            Some(serde_json::json!({ "tables": tables })),
        );
    } else {
        push_check(
            checks,
            "schema.tables",
            CheckStatus::Error,
            Some(format!("Missing tables: {}", missing_tables.join(", "))),
            Some(serde_json::json!({ "missing": missing_tables })),
        );
    }

    let required_columns: &[(&str, &[&str])] = &[
        (
            "issues",
            &[
                "id",
                "title",
                "status",
                "priority",
                "issue_type",
                "created_at",
                "updated_at",
            ],
        ),
        (
            "dependencies",
            &["issue_id", "depends_on_id", "type", "created_at"],
        ),
        (
            "comments",
            &["id", "issue_id", "author", "text", "created_at"],
        ),
        (
            "events",
            &["id", "issue_id", "event_type", "actor", "created_at"],
        ),
    ];

    let mut missing_columns = Vec::new();
    for (table, cols) in required_columns {
        let present = collect_table_columns(conn, table)?;
        let missing: Vec<&str> = cols
            .iter()
            .copied()
            .filter(|col| !present.iter().any(|p| p == col))
            .collect();
        if !missing.is_empty() {
            missing_columns.push(serde_json::json!({
                "table": table,
                "missing": missing,
            }));
        }
    }

    if missing_columns.is_empty() {
        push_check(checks, "schema.columns", CheckStatus::Ok, None, None);
    } else {
        push_check(
            checks,
            "schema.columns",
            CheckStatus::Error,
            Some("Missing required columns".to_string()),
            Some(serde_json::json!({ "tables": missing_columns })),
        );
    }

    Ok(())
}

fn check_integrity(conn: &Connection, checks: &mut Vec<CheckResult>) -> Result<()> {
    let result: String = conn.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if result.trim().eq_ignore_ascii_case("ok") {
        push_check(
            checks,
            "sqlite.integrity_check",
            CheckStatus::Ok,
            None,
            None,
        );
    } else {
        push_check(
            checks,
            "sqlite.integrity_check",
            CheckStatus::Error,
            Some(result),
            None,
        );
    }
    Ok(())
}

fn check_merge_artifacts(beads_dir: &Path, checks: &mut Vec<CheckResult>) -> Result<()> {
    let mut artifacts = Vec::new();
    for entry in beads_dir.read_dir()? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name.contains(".base.jsonl")
            || name.contains(".left.jsonl")
            || name.contains(".right.jsonl")
        {
            artifacts.push(name.to_string());
        }
    }

    if artifacts.is_empty() {
        push_check(checks, "jsonl.merge_artifacts", CheckStatus::Ok, None, None);
    } else {
        push_check(
            checks,
            "jsonl.merge_artifacts",
            CheckStatus::Warn,
            Some("Merge artifacts detected in .beads/".to_string()),
            Some(serde_json::json!({ "files": artifacts })),
        );
    }
    Ok(())
}

fn discover_jsonl(beads_dir: &Path) -> Option<PathBuf> {
    let issues = beads_dir.join("issues.jsonl");
    if issues.exists() {
        return Some(issues);
    }
    let legacy = beads_dir.join("beads.jsonl");
    if legacy.exists() {
        return Some(legacy);
    }
    None
}

fn check_jsonl(path: &Path, checks: &mut Vec<CheckResult>) -> Result<usize> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut total = 0usize;
    let mut invalid = Vec::new();
    let mut invalid_count = 0usize;

    for (idx, line) in reader.lines().enumerate() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        total += 1;
        if serde_json::from_str::<serde_json::Value>(trimmed).is_err() {
            invalid_count += 1;
            if invalid.len() < 10 {
                invalid.push(idx + 1);
            }
        }
    }

    if invalid.is_empty() {
        push_check(
            checks,
            "jsonl.parse",
            CheckStatus::Ok,
            Some(format!("Parsed {total} records")),
            Some(serde_json::json!({
                "path": path.display().to_string(),
                "records": total
            })),
        );
    } else {
        push_check(
            checks,
            "jsonl.parse",
            CheckStatus::Error,
            Some(format!(
                "Malformed JSONL lines: {invalid_count} (first: {invalid:?})"
            )),
            Some(serde_json::json!({
                "path": path.display().to_string(),
                "records": total,
                "invalid_lines": invalid,
                "invalid_count": invalid_count
            })),
        );
    }

    Ok(total)
}

fn check_db_count(
    conn: &Connection,
    jsonl_count: Option<usize>,
    checks: &mut Vec<CheckResult>,
) -> Result<()> {
    let db_count: i64 = conn.query_row("SELECT count(*) FROM issues", [], |row| row.get(0))?;

    if let Some(jsonl_count) = jsonl_count {
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        let db_count_usize = db_count as usize;
        if db_count_usize == jsonl_count {
            push_check(
                checks,
                "counts.db_vs_jsonl",
                CheckStatus::Ok,
                Some(format!("Both have {db_count} records")),
                None,
            );
        } else {
            push_check(
                checks,
                "counts.db_vs_jsonl",
                CheckStatus::Warn,
                Some("DB and JSONL counts differ".to_string()),
                Some(serde_json::json!({
                    "db": db_count,
                    "jsonl": jsonl_count
                })),
            );
        }
    } else {
        push_check(
            checks,
            "counts.db_vs_jsonl",
            CheckStatus::Warn,
            Some("JSONL not found; cannot compare counts".to_string()),
            Some(serde_json::json!({ "db": db_count })),
        );
    }

    Ok(())
}

/// Execute the doctor command.
///
/// # Errors
///
/// Returns an error if report serialization fails or if IO operations fail.
pub fn execute(json: bool) -> Result<()> {
    let mut checks = Vec::new();
    let beads_dir = Path::new(".beads");

    if !beads_dir.exists() {
        push_check(
            &mut checks,
            "beads_dir",
            CheckStatus::Error,
            Some("Missing .beads directory (run `br init`)".to_string()),
            None,
        );
        let report = DoctorReport {
            ok: !has_error(&checks),
            checks,
        };
        print_report(&report, json)?;
        std::process::exit(1);
    }

    check_merge_artifacts(beads_dir, &mut checks)?;

    let jsonl_path = discover_jsonl(beads_dir);
    let jsonl_count = if let Some(path) = jsonl_path.as_ref() {
        match check_jsonl(path, &mut checks) {
            Ok(count) => Some(count),
            Err(err) => {
                push_check(
                    &mut checks,
                    "jsonl.parse",
                    CheckStatus::Error,
                    Some(format!("Failed to read JSONL: {err}")),
                    Some(serde_json::json!({ "path": path.display().to_string() })),
                );
                None
            }
        }
    } else {
        push_check(
            &mut checks,
            "jsonl.parse",
            CheckStatus::Warn,
            Some("No JSONL file found (.beads/issues.jsonl or .beads/beads.jsonl)".to_string()),
            None,
        );
        None
    };

    let db_path = beads_dir.join("beads.db");
    if db_path.exists() {
        match Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
            Ok(conn) => {
                required_schema_checks(&conn, &mut checks)?;
                check_integrity(&conn, &mut checks)?;
                check_db_count(&conn, jsonl_count, &mut checks)?;
            }
            Err(err) => {
                push_check(
                    &mut checks,
                    "db.open",
                    CheckStatus::Error,
                    Some(format!("Failed to open DB read-only: {err}")),
                    Some(serde_json::json!({ "path": db_path.display().to_string() })),
                );
            }
        }
    } else {
        push_check(
            &mut checks,
            "db.exists",
            CheckStatus::Error,
            Some("Missing .beads/beads.db".to_string()),
            None,
        );
    }

    let report = DoctorReport {
        ok: !has_error(&checks),
        checks,
    };
    print_report(&report, json)?;

    if !report.ok {
        std::process::exit(1);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use tempfile::NamedTempFile;

    fn find_check<'a>(checks: &'a [CheckResult], name: &str) -> Option<&'a CheckResult> {
        checks.iter().find(|check| check.name == name)
    }

    #[test]
    fn test_check_jsonl_detects_malformed() -> Result<()> {
        let mut file = NamedTempFile::new().unwrap();
        std::io::Write::write_all(file.as_file_mut(), b"{\"id\":\"ok\"}\n")?;
        std::io::Write::write_all(file.as_file_mut(), b"{bad json}\n")?;

        let mut checks = Vec::new();
        let count = check_jsonl(file.path(), &mut checks).unwrap();
        assert_eq!(count, 2);

        let check = find_check(&checks, "jsonl.parse").expect("check present");
        assert!(matches!(check.status, CheckStatus::Error));

        Ok(())
    }

    #[test]
    fn test_required_schema_checks_missing_tables() {
        let conn = Connection::open_in_memory().unwrap();
        let mut checks = Vec::new();
        required_schema_checks(&conn, &mut checks).unwrap();

        let tables = find_check(&checks, "schema.tables").expect("tables check");
        assert!(matches!(tables.status, CheckStatus::Error));
    }
}
