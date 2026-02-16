//! `beads-lib` — In-process beads issue tracking library.
//!
//! Provides a standalone, SQLite-free API for managing beads issues.
//! Data is stored in memory and persisted via JSONL files.
//!
//! # Quick Start
//!
//! ```no_run
//! use beads_lib::{InMemoryStore, IssueUpdate, Status};
//! use beads_lib::model::Issue;
//!
//! // Load existing file
//! let mut store = InMemoryStore::open("path/to/.beads/issues.jsonl").unwrap();
//!
//! // Query
//! let ready = store.get_ready_issues(&Default::default(), Default::default());
//!
//! // Create
//! store.create_issue(&Issue { title: "New task".into(), ..Default::default() }, "agent").unwrap();
//!
//! // Update
//! store.update_issue("bd-abc123", &IssueUpdate { status: Some(Status::Closed), ..Default::default() }, "agent").unwrap();
//!
//! // Save back
//! store.save().unwrap();
//! ```

pub mod error;
pub mod jsonl;
pub mod model;
pub mod query;
pub mod store;
pub mod util;

pub use error::{BeadsError, Result};
pub use model::{Comment, Dependency, Event, Issue, Status};
pub use query::{IssueUpdate, ListFilters, ReadyFilters, ReadySortPolicy};
pub use store::InMemoryStore;
