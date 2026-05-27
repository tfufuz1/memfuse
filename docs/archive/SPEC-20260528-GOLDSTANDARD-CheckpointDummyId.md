# SPEC: Goldstandard Checkpoint Dummy ID
**Date**: 2026-05-28
**Author**: Lead Architect
**Target Agent**: 12 (Checkpoint Lead)

## context
In `crates/memfuse-checkpoint/src/lib.rs`, `PersistentCheckpointStore::create_checkpoint` uses `TxId::new(0)` internally to write checkpoint metadata to the store. Hardcoding `TxId(0)` can cause transaction isolation collisions with actual app-level data if a transaction ID 0 was used, and it bypasses robust TxBuffer isolation.

## objective
Remove the hardcoded `TxId::new(0)` from checkpoint persistence. Introduce a safe internal commit mechanism (`SystemTxId` or generate unique IDs, e.g., `TxId::new(u64::MAX)`) for internal metadata operations.

## instructions
1. Open `crates/memfuse-checkpoint/src/lib.rs`.
2. Locate the calls to `self.storage.put(TxId::new(0), ...)` and `self.storage.commit(TxId::new(0))`.
3. Introduce a strategy to handle internal transactions without colliding with user data. For instance, using `TxId::new(u64::MAX)` or adding a `SystemTxId` variant to the `memfuse_core` traits (if feasible without breaking other crates), OR simply generating a UUID/timestamp-based safe TxId. 
4. Update `create_checkpoint` and `drop_checkpoint` to use this safe transaction ID mechanism instead of `0`.

## verification
1. Run `cargo clippy -- -D warnings`.
2. Ensure `just triple-test` passes.
