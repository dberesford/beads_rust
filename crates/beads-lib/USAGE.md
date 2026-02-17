# beads-lib Usage

In-process beads issue tracking library — no SQLite, no daemon.

## Add dependency

In your consuming app's `Cargo.toml`:

```toml
[dependencies]
beads-lib = { path = "../beads_rust/crates/beads-lib" }
```

Adjust the relative path to match where `beads_rust` sits relative to your app.

## Example

```rust
use beads_lib::{InMemoryStore, IssueUpdate, Status};

// Load
let mut store = InMemoryStore::open(".beads/issues.jsonl")?;

// Query
let ready = store.get_ready_issues(&Default::default(), Default::default());

// Create
store.create_issue(&beads_lib::Issue {
    title: "New task".into(),
    ..Default::default()
}, "agent")?;

// Update
let id = store.resolve_id("abc123")?;
store.update_issue(&id, &IssueUpdate {
    status: Some(Status::Closed),
    ..Default::default()
}, "agent")?;

// Save
store.save()?;
```
