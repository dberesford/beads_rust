//! `SQLite` storage layer for `beads_rust`.
//!
//! This module provides the persistence layer using `SQLite` with:
//! - WAL mode for concurrent reads
//! - Transaction discipline for atomic writes
//! - Dirty tracking for JSONL export
//! - Blocked cache for ready/blocked queries
//!
//! # Submodules
//!
//! - [`events`] - Audit event storage (insertion, retrieval)

pub mod events;

pub use events::{
    EVENTS_TABLE_SCHEMA, count_events, get_all_events, get_events, init_events_table,
    insert_closed_event, insert_commented_event, insert_created_event, insert_deleted_event,
    insert_dependency_added_event, insert_dependency_removed_event, insert_event,
    insert_label_added_event, insert_label_removed_event, insert_reopened_event,
    insert_restored_event, insert_status_changed_event, insert_updated_event,
};
