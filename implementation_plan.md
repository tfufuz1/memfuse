# MVCC MemTable Refactoring

This plan implements point-in-time reads (Snapshot Isolation) by ensuring `MemTable` correctly handles multiple versions of keys and fixing a critical bug in version retrieval.

## Proposed Changes

### `memfuse-store`

#### [MODIFY] [memtable.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/memtable.rs)
- **Fix `get_at_seq`**: Update binary search to mask `TOMBSTONE_BIT` when comparing sequence numbers. Currently, it compares the clean search `seq_no` with the raw `seq_no` from the entry, which causes incorrect results when the tombstone bit is set (bit 63).
- **Optimize `iter_latest`**: Add a method to iterate only the latest version of each key. This will be used by `LsmStorage::flush` as per the "Correction" in the task description.

#### [MODIFY] [lsm.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/lsm.rs)
- **Update `flush`**: Use `memtable.iter_latest()` instead of `memtable.iter()` to ensure only the most recent version of each key is written to a single SSTable. This keeps the SSTable structure simple and consistent with standard LSM segments.
- **Note**: Versioning is still preserved because different versions of the same key will exist across different SSTables (or between the memtable and SSTables).

## Verification Plan

### Automated Tests
- `cargo test -p memfuse-store --lib memtable::tests::test_mvcc_tombstone_binary_search` (New test to be added)
- `cargo test -p memfuse-db --test transaction_isolation test_snapshot_stability`
- `cargo test -p memfuse-store` (Full suite)

### Manual Verification
- None required beyond automated tests.
