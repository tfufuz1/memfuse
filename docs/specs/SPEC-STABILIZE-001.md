# Atomic Spec: SPEC-STABILIZE-001 — MemFuse Stabilization Pass (Clippy & Safety)

## 1. Problem Statement
The project currently fails the `just test` gate due to several Clippy warnings and errors, which are treated as errors under the Sovereign Core Doctrine.
1.  **Deadlock Risk in HNSW**: `parking_lot` locks are held across `await` points in `HnswIndex::save`.
2.  **Dead Code**: `compaction_tx` in `LsmStorage` is never used.
3.  **Code Complexity**: `MemTable` uses a highly complex nested type for its entries.
4.  **Formatting**: Code base has formatting violations.

## 2. Proposed Solution

### 2.1 Fix HNSW Deadlock Risk
- Refactor `HnswIndex::save` to use `tokio::task::spawn_blocking` for the entire persistence logic.
- Inside `spawn_blocking`, use standard `std::fs::File` and `std::io::BufWriter` (blocking I/O is allowed in `spawn_blocking`).
- This allows acquiring `parking_lot::RwLockReadGuard` safely without holding them across async `await` points.
- Ensure `write_mutex` is still acquired asynchronously before spawning the blocking task to prevent concurrent modifications.

### 2.2 Fix Dead Code in LSM
- Rename `compaction_tx` to `_compaction_tx` if it's truly not needed, or better:
- Implement a `shutdown()` method for `LsmStorage` that sends a termination signal via `compaction_tx`.

### 2.3 Reduce Type Complexity in MemTable
- Define semantic type aliases in `memtable.rs`:
  - `type SequenceNumber = u64;`
  - `type TransactionId = u64;`
  - `type MemTableEntry = (SequenceNumber, Bytes, TransactionId);`
  - `type MemTableMap = BTreeMap<Bytes, Vec<MemTableEntry>>;`
- Update `MemTable` struct to use `MemTableMap`.

### 2.4 Formatting
- Run `cargo fmt --all`.

## 3. Technical Changes

### `crates/memfuse-index/src/hnsw.rs`
- Modify `save` method.

### `crates/memfuse-store/src/lsm.rs`
- Fix `compaction_tx` usage.

### `crates/memfuse-store/src/memtable.rs`
- Introduce type aliases.

## 4. Verification Plan (Triple-Test-Gate)
1. `cargo fmt --all -- --check` must pass.
2. `cargo clippy --all-targets -- -D warnings` must pass.
3. `cargo test --workspace` must pass.
4. `just test` must pass.
