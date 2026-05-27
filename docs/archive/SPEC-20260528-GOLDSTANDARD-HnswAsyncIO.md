# SPEC: Goldstandard Async I/O for HNSW
**Date**: 2026-05-28
**Author**: Lead Architect
**Target Agent**: 03 (Index Master)

## context
The `save()` method in `HnswIndexCore` (`crates/memfuse-index/src/hnsw.rs`) blocking thread execution by using `std::fs::File::create` and `std::io::BufWriter::write_all` within an `async fn`. This violates the Sovereign Core Doctrine against blocking I/O in async contexts.

## objective
Refactor the file saving and reading logic in `hnsw.rs` to not block the tokio executor.

## instructions
1. Open `crates/memfuse-index/src/hnsw.rs`.
2. Locate the `pub async fn save(&self, path: impl AsRef<std::path::Path>)` function.
3. Wrap the synchronous `std::fs` operations inside a `tokio::task::spawn_blocking` block OR rewrite the serialization logic natively using `tokio::fs::File` and `tokio::io::AsyncWriteExt`. Wrapping is preferred to preserve exact binary layout semantics if complicated logic is involved.
4. Ensure the same is done if there are other `std::fs` usages (e.g., in `load_mmap`).

## verification
1. Run `cargo clippy -- -D warnings`.
2. Ensure `just triple-test` passes without hanging.
