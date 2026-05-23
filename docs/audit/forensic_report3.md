# MemFuse Forensic Codebase Audit Report

**Date**: 2026-05-20  
**Phase**: Deep-Scan Forensic Audit  
**Target**: Complete `memfuse` workspace ([core](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/hnsw.rs#589-598), `store`, `index`, [db](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/lib.rs#424-436))

## 1. Compliance Audit

### Sovereign Core Doctrine Violations
The workspace was scanned exhaustively for `unwrap()`, `expect()`, `panic!()`, `unsafe`, and `std::fs` usages. 

- **Zero-Panic Policy**: ✅ **COMPLIANT** 
  - All `unwrap()` and `expect()` occurrences are strictly localized within `#[cfg(test)]` modules or `tests/` directories.
  - Production code correctly propagates errors via `memfuse_core::Result` and the `?` operator.
  - No explicit panics exist in the standard operational path.

- **Async Safety**: ✅ **COMPLIANT**
  - No blocking I/O (`std::fs`) in async execution paths. All disk operations correctly utilize Tokio's async FS.
  - `tokio::sync::Mutex` and `tokio::sync::RwLock` are deployed effectively. Synchronous locks (`std::sync::Mutex`, `parking_lot::RwLock`) are dropped strictly prior to `.await` barriers (e.g., in [memfuse-db/src/transaction.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/transaction.rs)).

- **Unsafe-Isolation**: ✅ **COMPLIANT**
  - Found 46 bounded `unsafe` blocks localized solely in [memfuse-index/src/distance.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/distance.rs).
  - All `unsafe` blocks are fully accounted for via `// SAFETY:` justifications, ensuring SIMD boundary checks and target feature derivations are properly isolated.

- **Warning Gates**: ✅ **COMPLIANT**
  - Codebase maintains clean hygiene under `#![forbid(unsafe_code)]` definitions outside specifically gated modules.

---

## 2. Architectural Deep-Scan: Critical Findings

A deep forensic architectural review of `memfuse-store` identified two **Critical Reliability Flaws** within the background compaction engine ([compaction.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/compaction.rs)).

### 🔴 CRITICAL: ARCH-COMPACTION-001 (MVCC Inversion via Non-Contiguous STCS)
**Subsystem:** `memfuse-store::compaction`
**Description:** The Size-Tiered Compaction Strategy (STCS) currently groups SSTables into "tiers" purely by file size irrespective of their chronological sequence in the disk hierarchy. 
**Impact:** If a non-contiguous group of SSTables is merged (e.g., merging the newest and oldest table but skipping the middle table), the resulting compacted table is written to the oldest index point. This pulls newer data behind older data in the LSM scan order. Older versions of a key in the skipped middle table will incorrectly shadow the newer versions pulled into the combined older table.
**Resolution:** Hard-enforce chronological contiguity. The selection logic [maybe_compact()](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/compaction.rs#76-158) must constrain SSTable grouping strictly to contiguous sliding windows. 

### 🔴 CRITICAL: ARCH-COMPACTION-002 (Tombstone Resurrection)
**Subsystem:** `memfuse-store::compaction`
**Description:** Tombstones are aggressively garbage-collected during an STCS pass if their sequence number is older than the `min_snapshot_seq`. 
**Impact:** Partial compaction windows do not encompass the entire SSTable hierarchy. If a tombstone is deleted in layer $L$, but the original overwritten data remains in layer $L+x$ (which was excluded from the compaction run), dropping the tombstone resurrects the old key in subsequent scans.
**Resolution:** A tombstone may only be reclaimed if it either (a) reaches the oldest underlying SSTable, or (b) the compaction window covers *all* SSTables from the tombstone's position down to the lowest file.

---

## 3. Recommended Remediation & Refactoring
1. Update [compaction.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/compaction.rs) candidate selection to enforce contiguous grouping rather than naive bucket aggregation.
2. Update tombstone processing logic in [compaction.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/compaction.rs) to require complete coverage guarantees before pruning a deleted entry.
3. Update [SPEC-20260505-WP-1.1-Compaction.md](file:///home/freddy/Arbeitsplatz/DEV/memfuse/docs/specs/SPEC-20260505-WP-1.1-Compaction.md) to enshrine these non-negotiable invariants to prevent regression.
