//! JSONL import/export for `beads_rust`.
//!
//! This module handles:
//! - Export: `SQLite` -> JSONL (for git tracking)
//! - Import: JSONL -> `SQLite` (for git clone/pull)
//! - Dirty tracking for incremental exports
//! - Collision detection during imports
