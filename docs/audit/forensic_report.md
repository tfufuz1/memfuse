# MemFuse Forensic Audit Report

> **Date:** 2026-05-20  
> **Auditor:** Principal Sovereign Systems Auditor  
> **Scope:** Full workspace — 10 crates, 53 Rust files, ~12,500 LoC  
> **Doctrine:** Zero-Panic / Sovereign Core / Triple-Test-Gate

---

## Executive Summary

| Crate | Grade | Critical | High | Medium | Low |
|:------|:-----:|:--------:|:----:|:------:|:---:|
| `memfuse-core` | **B+** | 1 | 1 | 0 | 0 |
| `memfuse-store` | **A-** | 0 | 1 | 1 | 1 |
| `memfuse-index` | **A** | 0 | 0 | 1 | 1 |
| `memfuse-db` | **A-** | 0 | 1 | 1 | 0 |
| `memfuse-text` | **A** | 0 | 0 | 0 | 0 |
| `memfuse-graph` | **A-** | 0 | 0 | 1 | 0 |
| `memfuse-runtime` | **A** | 0 | 0 | 0 | 0 |
| `memfuse-orchestrator` | **A** | 0 | 0 | 0 | 0 |
| `memfuse-py` | **A** | 0 | 0 | 0 | 0 |
| `memfuse-checkpoint` | **B** | 0 | 1 | 0 | 0 |

**Overall Grade: B+** — Solid architecture with well-documented invariants. A small number of correctness issues require attention before production hardening.

**Clippy Status:** ✅ `cargo clippy -- -D warnings` passes clean (0 warnings).

---

## Critical Findings

### CRIT-001: `DocId::from_key()` uses `.expect()` in production code

| Property | Value |
|:---------|:------|
| **File** | [types.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types.rs#L51) |
| **Severity** | 🔴 CRITICAL |
| **Category** | Zero-Panic Doctrine Violation |

```rust
// Line 51 — types.rs
Self::try_from_key(key).expect("Blake3 hash must be 32 bytes")
```

**Impact:** `DocId::from_key()` is called in **production hot-paths** throughout [collection.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/collection.rs) (lines 148, 211, 253, 522). While the blake3 hash invariant makes this practically infallible, the `.expect()` violates the **absolute** Zero-Panic doctrine.

**Fix:**
```diff
-    pub fn from_key(key: &str) -> Self {
-        Self::try_from_key(key).expect("Blake3 hash must be 32 bytes")
+    pub fn from_key(key: &str) -> Result<Self> {
+        Self::try_from_key(key)
     }
```

> [!CAUTION]
> This is a **breaking API change** — all 4 call sites in [collection.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/collection.rs), plus [load_index()](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/collection.rs#508-528), must be updated to propagate the `?`.

---

### CRIT-002: `ResourceTracker::consume_memory()` TOCTOU Race

| Property | Value |
|:---------|:------|
| **File** | [types.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types.rs#L303-L316) |
| **Severity** | 🟠 HIGH |
| **Category** | Concurrency / Data Race |

```rust
let current = self.memory_used.fetch_add(bytes, Ordering::SeqCst);
if current + bytes > self.budget.memory_limit {
    self.memory_used.fetch_sub(bytes, Ordering::SeqCst);  // rollback
    return Err(...);
}
```

**Problem:** Two threads can both `fetch_add` past the limit simultaneously. Thread A adds 100MB, thread B adds 100MB. Both see [current](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/tx_buffer.rs#323-363) as pre-increment. Both pass the check. Memory tracker now holds 200MB of phantom usage before the rollback races complete.

**Fix:** Use `compare_exchange` loop:
```rust
loop {
    let current = self.memory_used.load(Ordering::Acquire);
    if current + bytes > self.budget.memory_limit {
        return Err(MemFuseError::MemoryBudgetExceeded { ... });
    }
    if self.memory_used.compare_exchange(
        current, current + bytes,
        Ordering::AcqRel, Ordering::Relaxed
    ).is_ok() {
        return Ok(());
    }
}
```

---

## Architectural Findings

### ARCH-001: Circular Dependency — `memfuse-checkpoint` ↔ `memfuse-db`

| Property | Value |
|:---------|:------|
| **Severity** | 🟠 HIGH |
| **Category** | DAG Hierarchy Violation |

The declared architecture mandates `core → {store, index} → db`. However:

```
memfuse-db       depends on: memfuse-checkpoint
memfuse-checkpoint depends on: memfuse-db
```

This creates a **circular dependency** that violates the Hierarchical DAG doctrine. Cargo resolves this today only because the cycle flows through `[dev-dependencies]`, but this is architecturally fragile.

**Fix:** Extract a shared `checkpoint-types` crate, or move the checkpoint trait definitions into `memfuse-core`.

---

### ARCH-002: `memfuse-text` depends on `memfuse-store`

`memfuse-text` has a direct dependency on `memfuse-store`. Per the DAG hierarchy, `memfuse-text` should only depend on `memfuse-core`. The storage layer should be injected via the [StorageEngine](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs#46-99) trait, not via a concrete [LsmStorage](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/lsm.rs#103-117) import.

---

### ARCH-003: HNSW [do_delete()](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/hnsw.rs#543-580) Entry-Point Replacement is O(n)

| Property | Value |
|:---------|:------|
| **File** | [hnsw.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/hnsw.rs#L564-L569) |
| **Severity** | 🟡 MEDIUM |

When the entry point is deleted, the code scans **all nodes** to find the highest-layer replacement:

```rust
for (i, node) in nodes.iter().enumerate() { ... }
```

At 1M vectors, this is a 1M-iteration linear scan while holding 3 write locks (`entry_point`, `nodes`, `deleted_nodes`). This blocks all concurrent reads/writes.

**Fix:** Maintain a secondary heap/sorted structure of (layer, node_idx) to find the new entry point in O(log n).

---

## Hardening Recommendations

### HARD-001: WAL Replay Unbounded Memory

| Property | Value |
|:---------|:------|
| **File** | [wal.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/wal.rs#L234) |
| **Severity** | 🟡 MEDIUM |

`Wal::replay()` reads the entire WAL file into a single `Vec<u8>` via `read_to_end()`. A malformed or maliciously large WAL (e.g., 128MB max) could cause OOM on constrained systems.

**Fix:** Stream-parse the WAL with a buffered reader instead of loading fully into memory.

### HARD-002: No Key/Value Size Limits

| Property | Value |
|:---------|:------|
| **Severity** | 🟡 MEDIUM |

Neither `LsmStorage::put()` nor `Collection::insert()` validate the size of keys or values. A single 1GB value would:
1. Pass budget checks (checked at aggregate level)
2. Be serialized into a single WAL entry
3. Be held in-memory in the MemTable

**Fix:** Add `MAX_KEY_SIZE` (e.g., 64KB) and `MAX_VALUE_SIZE` (e.g., 16MB) guards to `LsmStorage::put()`.

### HARD-003: Collection Name in Storage Key Allows Prefix Collision

| Property | Value |
|:---------|:------|
| **File** | [collection.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/collection.rs#L72-L76) |
| **Severity** | 🟢 LOW |

Collection `"a"` uses prefix `__col:a:\x00` but there is no validation preventing a collection named `"a:\x00"` or containing null bytes (the only validation is alphanumeric + `_` + `-`), so this is mitigated by the existing validation. Confirmed safe.

---

## Doctrine Compliance Matrix

| Rule | Status | Notes |
|:-----|:------:|:------|
| `#![forbid(unsafe_code)]` all crates | ⚠️ | 8/10 have `forbid`, `memfuse-store` and `memfuse-index` use `deny` with documented exceptions |
| Zero `.unwrap()` in production | ❌ | `DocId::from_key()` violates (CRIT-001) |
| Zero `std::fs` in async | ✅ | Only in test code ([encryption_test.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/tests/encryption_test.rs)) |
| `unsafe` only in SIMD/FFI with SAFETY | ✅ | 30+ SAFETY comments in [distance.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/distance.rs), [mmap.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/mmap.rs) is a stub |
| `clippy -D warnings` green | ✅ | 0 warnings |
| DAG hierarchy respected | ❌ | Circular dep [checkpoint](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs#73-77) ↔ [db](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/lib.rs#424-436) (ARCH-001) |
| Every public API has a test | ✅ | All trait implementations are tested |

---

## `unsafe` Block Audit

All `unsafe` blocks are in [distance.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/distance.rs) (SIMD) and [mmap.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/mmap.rs) (stub).

| Block | Location | Safety Proof | Verdict |
|:------|:---------|:-------------|:-------:|
| AVX2 f32 dot/cos/euc | `distance.rs:266-388` | `is_x86_feature_detected!` + `i+8<=n` bounds | ✅ Sound |
| AVX-512 f32 dot/cos/euc | `distance.rs:415-551` | `is_x86_feature_detected!` + `i+16<=n` bounds | ✅ Sound |
| AVX2 u8 dot/euc/cos | `distance.rs:717-870` | Same guards, `i+32<=n` bounds | ✅ Sound |
| Horizontal sums | `distance.rs:396-405,542-551` | Called only from `#[target_feature]` fns | ✅ Sound |
| [mmap.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/mmap.rs) | `mmap.rs:17-21` | Stub — no actual unsafe operations | ✅ Inert |

---

## Positive Findings

1. **Commit serialization** via `commit_mutex` correctly prevents MVCC snapshot inversion
2. **WAL HMAC-SHA256 verification** on replay prevents silent data corruption
3. **Compaction tombstone GC** correctly respects [min_active_seqno](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/snapshot.rs#60-65) from [SnapshotRegistry](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/snapshot.rs#27-31)
4. **HNSW NaN/Inf guard** at insert boundary prevents heap corruption
5. **`total_cmp()` in `Candidate::cmp()`** eliminates NaN-related non-determinism
6. **[random_layer()](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/hnsw.rs#222-234) clamp** to 32 prevents OOM from pathological `ln()` values
7. **TxBuffer sharding** design is correct: single-shard-per-tx prevents deadlocks
8. **SnapshotGuard RAII pattern** prevents snapshot leaks
9. **Compaction atomic swap** is correct: merge outside lock, swap inside write-lock
10. **Orphan reaper** with configurable timeout prevents transaction leaks

---

## Action Items (Priority Order)

| # | Action | Priority | Effort |
|:--|:-------|:--------:|:------:|
| 1 | Fix `DocId::from_key()` → return [Result](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/lib.rs#67-75) (CRIT-001) | 🔴 | S |
| 2 | Fix `ResourceTracker::consume_memory()` CAS loop (CRIT-002) | 🟠 | S |
| 3 | Break `checkpoint ↔ db` cycle (ARCH-001) | 🟠 | M |
| 4 | Decouple `memfuse-text` from `memfuse-store` (ARCH-002) | 🟡 | M |
| 5 | Add max key/value size guards (HARD-002) | 🟡 | S |
| 6 | Stream-parse WAL replay (HARD-001) | 🟡 | M |
| 7 | Optimize HNSW EP replacement to O(log n) (ARCH-003) | 🟡 | M |
