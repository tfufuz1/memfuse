# Benchmarks & Optimizations (AGENT:09)

## Summary
Initialized benchmark suite for distance metrics and performed two major performance optimizations in the workspace.

## Distance Metric Benchmarks
Created `crates/memfuse-index/benches/distance_bench.rs` to compare Scalar vs SIMD implementations.
SIMD (AVX2/AVX-512) provides significant speedup for high-dimensional vector operations.

## Optimizations

### 1. `Collection::namespaced_key` (memfuse-db)
- **Problem:** Frequent allocations of `Vec<u8>` when generating keys for LSM-Tree. Each call would clone the prefix and then extend it.
- **Solution:** Switched to `Vec::with_capacity(prefix.len() + 1 + key.len())` to perform a single allocation.
- **Impact:** Reduced allocation churn in hot path for `insert`, `get`, `delete`, and `search`.

### 2. `HnswNode` Vector Storage (memfuse-index)
- **Problem:** `HnswNode` stored vectors as `Vec<f32>`, leading to expensive clones during neighbor selection, rebuilding, and insertion.
- **Solution:**
    - Changed `HnswNode.vector` to `Arc<[f32]>`.
    - Used `Arc::clone` instead of `.to_vec()` or `.clone()` where possible.
    - Used `with_capacity` for `visited` set and `results` heap in `search_layer` to minimize reallocations.
- **Impact:** Significant reduction in memory allocations and CPU time during HNSW graph traversals and index maintenance.

## Verification
All optimizations verified via:
1. `cargo test --workspace` (No regressions)
2. `cargo clippy --all-targets` (Zero warnings)
3. `cargo bench -p memfuse-index --no-run` (Benchmark compilation)
