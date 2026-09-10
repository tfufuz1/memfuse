# Audit Report: LSM Engine Recovery & Fault Tolerance (`memfuse-store`)

**Role**: Principal Senior Rust Storage Engine Architect
**Date**: 2026-09-10
**Scope Crates & Files**:
- `crates/memfuse-store/src/wal.rs`
- `crates/memfuse-store/src/memtable.rs`
- `crates/memfuse-store/src/sstable.rs`
- `crates/memfuse-store/src/compaction.rs`
- `crates/memfuse-store/src/lsm.rs`

---

## Executive Summary

An architectural and implementation audit was conducted across the `memfuse-store` LSM-Tree engine focusing on Write-Ahead Log (WAL) durability, `fsync` discipline, crash-recovery mechanisms, atomic commit guarantees, and Tombstone Garbage Collection during compaction.

Fault injection integration tests were implemented and verified in `crates/memfuse-store/tests/fault_injection_recovery.rs`. All unit and integration test suites pass cleanly (`cargo test -p memfuse-store --all-features`).

---

## Error Class Analysis & Remediation Verification

### 1. Dirty Read / Non-Repeatable Read
- **Problem**: Reading uncommitted MemTable data outside the active transaction snapshot.
- **Verification & Safeguards**:
  - `LsmStorage::commit` holds `commit_mutex` to serialize sequence assignment (`fetch_add`) and WAL appending before applying mutations to the active `MemTable`.
  - Point lookups (`get`, `get_at_seq`) and range scans (`scan_prefix_at`) enforce the **Single Load Rule** for `last_committed_tx`. The read snapshot is fixed at the start of traversal to ensure that uncommitted or concurrently commited writes beyond the snapshot boundary are filtered out.

### 2. Atomicity Failure / Missing Rollback
- **Problem**: Partial write access to SSTables or WAL during a process crash or I/O failure.
- **Verification & Safeguards**:
  - `LsmStorage::commit` executes Phase 2 (Group Commit to WAL) before Phase 3 (MemTable write). If `Wal::append_batch` returns an I/O error, the `last_hmac` chain link is restored and `rollback_to_tx_locked` executes physical state truncation back to the last committed transaction.
  - SSTable writes follow the atomic temporary file pattern: output is written to `sst-*.tmp`, `sync_all()` is invoked on the file handle, renamed to `.sst`, and `fsync_parent_dir()` flushes the parent directory entry before `sstables` write-lock registration.

### 3. WAL Corruption / Broken Hash Chain
- **Problem**: Corruption of WAL segment checksums or undetected truncated records at log end.
- **Verification & Safeguards**:
  - WAL records use length-prefixed framing: `u32` payload size, `u32` CRC32 checksum, `u64` `seq_no`, 32-byte HMAC checksum, 32-byte `prev_hmac` (chain link), and operation payload (`tx_id`, `key`, `value`).
  - During `replay()`:
    - CRC32 mismatches or deserialization errors at file end (when `pos >= file_size`) are safely recognized as truncated tail writes from abrupt process termination and cleanly ignored.
    - Any payload corruption or bit-flip prior to the file tail raises an explicit `MemFuseError::WalCorruption`.
    - Every entry's HMAC is verified against the derived per-file integrity key and `prev_hmac` link, guaranteeing cryptographic tamper-detection.

### 4. Write / Read / Space Amplification
- **Problem**: Disproportionate I/O overhead due to inefficient compaction triggering.
- **Verification & Safeguards**:
  - Size-Tiered Compaction Strategy (STCS) in `CompactionEngine` groups SSTables into size tiers with a default ratio of `4.0` and a threshold `min_sstables_per_tier = 4`.
  - Candidate selection sorts SSTables chronologically by `max_seq` to guarantee write order preservation and bounded read amplification.

### 5. Tombstone Leak / Accumulation
- **Problem**: Missing tombstone scrubbing during compaction leading to unbounded disk growth.
- **Verification & Safeguards**:
  - `CompactionEngine::merge_sstables` checks `should_gc_tombstone`:
    - `is_tombstone == true`
    - `is_full_compaction == true` (ensuring no older SSTables outside the current compaction tier hold stale values for the key)
    - `raw_seq < min_snapshot_seq` (ensuring no active snapshot pin in `SnapshotRegistry` references the deleted version)
  - Tombstones meeting all three criteria are physically discarded from the output SSTable.

### 6. Truncation Crash
- **Problem**: Startup crashes caused by partially written SSTables or empty data segments.
- **Verification & Safeguards**:
  - On `LsmStorage::new()`, a startup directory recovery scan discovers and unlinks any orphaned `.tmp` files (`sst-*.tmp`, `SALT.tmp.*`, `.wal_integrity_key.tmp.*`, `.uuid.tmp.*`).
  - Fault injection test `test_fault_injection_sstable_temp_files_cleaned_up_on_open` verifies orphaned temp files are automatically removed upon startup.

---

## Fault Injection Test Results

All fault injection scenarios were executed via `cargo test -p memfuse-store --test fault_injection_recovery`:

| Test Case | Scenario | Result |
|---|---|---|
| `test_fault_injection_wal_tail_truncation_recovery` | Partial record truncation at WAL log tail | **PASS** (replaces clean entries, discards truncated tail gracefully) |
| `test_fault_injection_wal_middle_bitflip_detected` | Bit-flip injection in middle WAL payload | **PASS** (fails startup with `WalCorruption` / CRC mismatch error) |
| `test_fault_injection_sstable_temp_files_cleaned_up_on_open` | Orphaned `.sst.tmp` & `SALT.tmp` files | **PASS** (cleaned up automatically during startup scan) |

---

## Verification Summary

- `cargo test -p memfuse-store --all-features`: **135 passed, 0 failed**.
- `cargo clippy -p memfuse-store --all-features -- -D warnings`: **0 errors, 0 warnings**.
- `cargo fmt --check`: **0 formatting issues**.
