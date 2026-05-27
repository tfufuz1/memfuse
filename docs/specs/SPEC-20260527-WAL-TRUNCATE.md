# Atomic Spec: WAL Truncation for Rollback Consistency

**ID:** SPEC-20260527-WAL-TRUNCATE
**Status:** Draft
**Author:** Agent 02 (Store Engineer)
**Crate:** `memfuse-store`

## 1. Problem Statement
The `LsmStorage::rollback_to_tx` method currently only reverts the in-memory `MemTable` and `next_seq_no`. The physical Write-Ahead Log (WAL) file remains unchanged. Upon system restart, the `replay()` logic will read all entries from the WAL, effectively "re-applying" the transactions that were supposed to be rolled back.

## 2. Goals
- Add a `truncate` capability to the `Wal` struct to physically shorten the log file.
- Update `LsmStorage::rollback_to_tx` to use this truncation to ensure disk-level consistency.
- Ensure that the WAL's in-memory state (size, last HMAC) is correctly updated after truncation.

## 3. Technical Requirements

### 3.1. WAL Truncation (`wal.rs`)
- **Method:** `pub async fn truncate(&self, offset: u64) -> Result<()>`
- **Action:** 
    1. Lock the WAL file mutex.
    2. Use `tokio::fs::File::set_len(offset)` to truncate the file.
    3. Update `self.size` (AtomicU64).
    4. Seek to the new end of the file to ensure subsequent appends happen at the correct position.
    5. **Re-calculate `last_hmac`**: Since we are removing entries, the chain link for the next append must be the HMAC of the *new* last entry. This requires a partial replay or tracking HMACs.

### 3.2. LSM Rollback Integration (`lsm.rs`)
- **Method:** `rollback_to_tx(target_tx: TxId)`
- **Action:**
    1. During the initial WAL replay, track the byte offset at which each entry starts and ends.
    2. Find the *highest* offset that belongs to an entry with `tx_id <= target_tx`.
    3. Call `wal.truncate(new_offset)`.

## 4. Constraint Checklist & Sovereign Core Doctrine
- [x] No `std::fs`. Use `tokio::fs`.
- [x] No `.unwrap()`. Propagate `MemFuseError`.
- [x] Atomic updates to in-memory state.
- [x] Thread-safe access via existing Mutexes.

## 5. Verification Plan
- **Test Case:** `test_wal_physical_truncate`
    1. Write 3 entries to WAL.
    2. Truncate to the end of the 2nd entry.
    3. Verify file size on disk.
    4. Replay WAL and verify only 2 entries exist.
    5. Append a 4th entry and verify hash chaining remains valid.
- **Integration Test:** `test_lsm_rollback_persistence`
    1. Commit Tx1, Tx2.
    2. Rollback to Tx1.
    3. Restart (re-instantiate `LsmStorage`).
    4. Verify only Tx1 data is present.
