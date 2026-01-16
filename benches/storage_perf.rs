//! Storage performance benchmarks.
//!
//! Run with: cargo bench

use criterion::{Criterion, criterion_group, criterion_main};

#[allow(clippy::missing_const_for_fn)]
fn storage_benchmark(_c: &mut Criterion) {
    // Placeholder - benchmarks will be implemented with storage layer
}

criterion_group!(benches, storage_benchmark);
criterion_main!(benches);
