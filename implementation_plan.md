# Implementation Plan

## Goal Description
Fix the two critical architectural logic flaws found during the forensic audit of [memfuse-store/src/compaction.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/compaction.rs) to ensure full LSM MVCC integrity and prevent data corruption (MVCC inversion and Tombstone Resurrection).

## Proposed Changes

### `memfuse-store`
#### [MODIFY] [compaction.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/compaction.rs)
- **Fix ARCH-COMPACTION-001 (Contiguous Generation)**: Rewrite the [maybe_compact()](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/compaction.rs#76-158) logic where size tiers are grouped. Replace the naive `tiers` bucket mapping with a sliding window mechanism that scans `ssts` (from newest to oldest) and evaluates contiguous windows of SSTables. Ensure that any candidate grouping represents a strictly contiguous sequence of SSTable indices.
- **Fix ARCH-COMPACTION-002 (Tombstone Resurrection)**: Modify the GC logic (`is_tombstone && raw_seq < min_snapshot_seq`). A tombstone can only be safely omitted from the output SSTable if the compaction process covers the **deepest (oldest) end** of the [sstables](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/compaction.rs#216-269) bounds (e.g. `indices` includes index 0, assuming index 0 is oldest), OR we guarantee no older table has the key (which is too expensive generically for STCS without a bloom filter check on all older files). The simplest bullet-proof path: Only GC tombstones if `insertion_point == 0` (meaning this compaction run incorporates the oldest SSTable in the system) or enforce that tombstones are flushed down.

### `docs/specs`
#### [MODIFY] `SPEC-20260505-WP-1.1-Compaction.md`
- Update the STCS group mapping algorithm under section 5 ("Implementierungsdetail") to mandate chronologically adjacent windows.
- Update the tombstone drop rules to explicitly state that `seq_no < min_snapshot_seq` is only valid if the compaction includes the deepest/oldest SSTable in the system.

## Verification Plan

### Automated Tests
- Run the full workspace unit tests `cargo test --workspace` to ensure existing stress tests and compaction tests continue to pass with the corrected logic mapping.

### Manual Verification
- Re-run `clippy -D warnings` to ensure zero-warnings are maintained as part of the Sovereign Core doctrine.
