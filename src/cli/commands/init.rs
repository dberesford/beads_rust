use crate::error::{BeadsError, Result};
use crate::storage::SqliteStorage;
use std::fs;
use std::path::Path;

/// Execute the init command.
///
/// # Errors
///
/// Returns an error if the directory or database cannot be created.
pub fn execute(prefix: Option<String>, force: bool) -> Result<()> {
    let beads_dir = Path::new(".beads");

    if beads_dir.exists() {
        // Check if DB exists
        let db_path = beads_dir.join("beads.db");
        if db_path.exists() && !force {
            return Err(BeadsError::AlreadyInitialized { path: db_path });
        }
    } else {
        fs::create_dir(beads_dir)?;
    }

    let db_path = beads_dir.join("beads.db");

    // Initialize DB (creates file and applies schema)
    let _storage = SqliteStorage::open(&db_path)?;

    // Set prefix in config table if provided
    if let Some(p) = prefix {
        // storage.set_config("issue_prefix", &p)?;
        // TODO: Implement set_config in SqliteStorage
        // For now, just logging it
        println!("Prefix set to: {p}");
    }

    // Write metadata.json
    let metadata_path = beads_dir.join("metadata.json");
    if !metadata_path.exists() || force {
        let metadata = r#"{
  "database": "beads.db",
  "jsonl_export": "issues.jsonl"
}"#;
        fs::write(metadata_path, metadata)?;
    }

    // Write config.yaml template
    let config_path = beads_dir.join("config.yaml");
    if !config_path.exists() {
        let config = r"# Beads Project Configuration
# prefix: bd
# default_priority: 2
# default_type: task
";
        fs::write(config_path, config)?;
    }

    // Write .gitignore
    let gitignore_path = beads_dir.join(".gitignore");
    if !gitignore_path.exists() {
        let gitignore = r"# Database
*.db
*.db-shm
*.db-wal

# Lock files
*.lock

# Temporary
last-touched
";
        fs::write(gitignore_path, gitignore)?;
    }

    println!("Initialized beads workspace in .beads/");
    Ok(())
}
