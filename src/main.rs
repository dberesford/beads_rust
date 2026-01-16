//! `beads_rust` (br) - Agent-first issue tracker
//!
//! A Rust port of the classic beads issue tracker with `SQLite` + JSONL hybrid storage.
//! Non-invasive design: no automatic git hooks, no daemon, no background processes.

use beads_rust::run;

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
