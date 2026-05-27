# MemFuse Forensic Codebase Audit Report (v2.0-Alpha)

**Date**: 2026-05-27
**Scope**: 11 Crates, ~11K LoC
**Objective**: Hardening validation, architectural specification, and gap analysis for air-gapped LLM Agent operation.

## Executive Summary

The MemFuse v2.0-Alpha system demonstrates a highly resilient, heavily optimized sovereign core architecture. The 4-Layer Dependency DAG (L0 Kernel → L1 Sub-Engines → L2 Orchestration → L3 Interface) is strictly adhered to, with no cyclic dependencies. Asynchronous safety and memory-mapped optimizations are functional. However, a few critical architectural stumps/skeletons were identified that represent functional gaps or tech-debt risks.

---

## 1. L0 Kernel (`memfuse-core`)
**Status**: ✅ Stabilized & Compliant

- **Traits & Isolation**: The core enforces `StorageEngine`, `VectorIndex`, `TextIndex`, and `GraphIndex` traits flawlessly. This enables modular dependency injection at L2.
- **TxBuffer & MVCC**: Uses `TOMBSTONE_BIT` (bit 63) dynamically in Sequence numbers (`memtable.rs`, `snapshot.rs`). The `TxBuffer` implements a lock-free sharded ring buffer with a 60-sec timeout and orphan reaper. **Validated as thread-safe**.
- **Error Handling**: `MemFuseError` aggregates all domain failures, preventing untracked panics. `#![deny(unsafe_code)]` is enforced.

## 2. L1 Sub-Engines
**Status**: 🟡 Stable but features missing

### `memfuse-store` (LSM-Tree)
- **WAL (Write-Ahead Log)**: Employs HMAC-SHA256 hash-chaining to detect tampering. 
  - ⚠️ **Issue**: The `rollback_to_tx()` function only performs in-memory rollback; it **does not truncate the WAL** on disk. This could cause orphaned edits to resurrect on restart.
- **SSTables**: Successfully memory-maps (`mmap2`) `.sst` files. Optimizes pre-checks via a mathematical Bloom Filter implementation.
- **Compaction**: Size-Tiered Compaction Strategy (STCS) reliably drops tombstones when no MVCC snapshots lock them (validated by rigorous integration test `test_compaction_stress_and_gc`).

### `memfuse-index` (HNSW)
- **Architecture**: Heavily optimized implementation featuring SIMD distance calculations (portable_simd, AVX-512, AVX2).
- **Quantization**: SQ8 Scalar Quantization reduces memory payload by factor ~4, decompressing inline inside distance matrices.
- **Safety Violation**: `unsafe` is used aggressively in `distance.rs`, but appropriately documented using `// SAFETY: ...` block annotations to justify manual bounds checks.

### `memfuse-text` & `memfuse-graph`
- **memfuse-text**: Accurate BM25 scoring algorithm with morphological tokenization (`GermanCompoundSplitter`).
- **memfuse-graph**: Fully upgraded from Scaffold. CSR (Compressed Sparse Row) implementation correctly applies graph breadth-first traversal with exponential score decay.

### `memfuse-crypto`
- **Architecture**: AES-256-GCM encryption with HKDF key derivation operates reliably for resting data.
- 🛑 **Vulnerability Skeleton**: `EncryptedWal::encrypt_chunk()` in `wal_crypto.rs` is a stub. It currently returns plaintext. This breaks "Encryption at rest" claims for the WAL layer.

## 3. L2 Orchestrator (`memfuse-db`)
**Status**: ✅ Atomic Guarantee Validated

- **2-Phase Commit**: `DbTransaction` (in `transaction.rs`) employs a robust intent-writing schema (`pending` -> LSM Commit -> HNSW Commit -> LSM `committed`).
- **Durable Rollback**: In the event of a crash during HNSW commit, the system initiates a Compensating Transaction (max 3 retries, exponentially backed) erasing LSM updates.
- **Hybrid Search**: `fusion.rs` effectively implements Reciprocal Rank Fusion (RRF) across BM25 textual signal and HNSW spatial nearest-neighbors.

## 4. L3 Interface (`memfuse-py`)
**Status**: ✅ Stabilized

- Exposes Thread-Safe asynchronous python implementations (`pyo3` and `tokio`).
- Wraps RRF routines neatly inside `PyMemFuse.hybrid_search`.

---

## Actionable Recommendations & Stubs for Implementation

This section defines the "Skeletons" that a subsequent LLM Agent must implement:

1. **`memfuse-crypto` - WAL Encryption (CRIT-002)**
   - **File**: `crates/memfuse-crypto/src/wal_crypto.rs`
   - **Task**: Replace the `encrypt_chunk` stub with actual block-chunked AES-256-GCM encryption based on the existing `crypto::KeyManager`.
2. **`memfuse-store` - WAL Truncation (HIGH-003)**
   - **File**: `crates/memfuse-store/src/lsm.rs` & `wal.rs`
   - **Task**: The `lsm.rollback_to_tx` method fails to truncate on-disk `.log` files when an aborted `TxId` represents the end of the log sequence. Needs file size truncation logic.
3. **`memfuse-db` - Async Trait Boxing Overhead (OPT-001)**
   - **File**: `crates/memfuse-core/src/traits.rs`
   - **Task**: Migrate from the `#[async_trait]` macro (dynamic allocation overhead) to native `async fn` traits now stabilized in rust 1.75+.

## Conclusion
The repository strictly respects zero-panic execution and concurrency bounds. The codebase provides a rock-solid platform for local AI agents, pending resolution of the WAL encryption bypass and on-disk truncation bug.
