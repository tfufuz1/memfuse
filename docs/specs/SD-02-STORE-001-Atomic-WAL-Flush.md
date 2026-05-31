# Atomic Spec: SD-02-STORE-001 Atomic WAL Flush

## 1. Problem Statement
In `crates/memfuse-store/src/lsm.rs`, the `flush()` method currently deletes the old WAL file *before* the new SSTable has been successfully created and persisted to disk.

```rust
// Current (Simplified)
let old_wal_path = old_wal.path().to_path_buf();
drop(old_wal);
tokio::fs::remove_file(&old_wal_path).await?; // <--- DELETED TOO EARLY

let mut builder = SstableBuilder::create(...).await?;
// ... write entries ...
builder.finish().await?; // <--- Persisted here
```

If the process crashes or fails during the `builder` phase, the data that was only present in `old_wal` and `old_memtable` (in-memory) is lost.

## 2. Proposed Solution
Reorder the operations to follow the "Commit-then-Cleanup" principle.

1.  **Atomic Memory Swap**: Swap the active MemTable and WAL with new ones. Move the old MemTable to `immutable_memtables`.
2.  **SSTable Creation**: Build the SSTable from the `old_memtable`.
3.  **Finish & Sync**: Ensure the SSTable is fully persisted to disk (`builder.finish()`).
4.  **Registration**: Add the new `SstableReader` to the active `sstables` and remove the `old_memtable` from `immutable_memtables`.
5.  **WAL Cleanup**: Delete the old WAL file only after the above steps succeed.

## 3. Technical Changes
### `crates/memfuse-store/src/lsm.rs`
Modify `async fn flush(&self)`:
- Move the `tokio::fs::remove_file` call to the very end of the function.
- Ensure that if SSTable creation fails, the `immutable_memtables` and the old WAL (which is still on disk) remain intact (though the in-memory `old_wal` handle is dropped, the file stays until successfully flushed).
- *Note*: If we crash, the `immutable_memtables` is lost (in-memory), but the old WAL file on disk will be discovered during the next startup/replay.

## 4. Verification Plan (TDD)
- **Test Case**: `test_flush_durability_on_failure`
  - Fill a MemTable with data.
  - Trigger a flush.
  - Mock/Simulate a failure during SSTable writing (e.g., by injecting an error or just checking file existence).
  - Verify that the WAL file still exists if the SSTable was not finished.
- **Integration Test**: `test_recovery_after_partial_flush`
  - Start DB, write data.
  - Start flush.
  - Interrupt (conceptually).
  - Restart and verify data is still there from WAL replay.
