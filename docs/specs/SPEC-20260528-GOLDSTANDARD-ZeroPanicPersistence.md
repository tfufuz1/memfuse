# SPEC: Goldstandard Zero-Panic Persistence
**Date**: 2026-05-28
**Author**: Lead Architect
**Target Agent**: 13 (Debt Hunter)

## context
The `memfuse-index/src/persistence.rs` file contains 17 instances of `.unwrap()` on `try_into()` when parsing headers and nodes. This violates the Sovereign Core Zero-Panic doctrine.

## objective
Eradicate all instances of `.unwrap()` in `persistence.rs`. If a byte slice is too small to be parsed, return `Err(MemFuseError::Storage(...))`.

## instructions
1. Open `crates/memfuse-index/src/persistence.rs`.
2. Locate all `unwrap()` calls on `try_into()` (e.g. `u64::from_le_bytes(bytes[24..32].try_into().unwrap())`).
3. Replace them with proper error handling using `try_into().map_err(|_| MemFuseError::Storage("Corrupt index (invalid bytes)".into()))?`. Note that `NodeRecord::from_bytes` should probably return a `Result<Self>` instead of panicking.
4. If `NodeRecord::from_bytes` signature changes to `Result`, bubble up the errors in `MmapIndex::get_node_record` and wherever else it is called.

## verification
1. `just triple-test` ensures no existing tests fail.
2. `cargo clippy -- -D warnings` must pass.
3. Search for `.unwrap()` in `memfuse-index/src/persistence.rs` — the count must be 0.
