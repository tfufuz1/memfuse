# MemFuse — Spec Coverage Matrix
**Date:** 2026-05-08

## Coverage by Module

| Module | Spec File | Coverage | Befund |
|--------|-----------|----------|--------|
| `memfuse-core/error.rs` | WP-0.0 | ✅ FULL | MemFuseError stable, thiserror-based |
| `memfuse-core/types.rs` | WP-0.0 | ✅ FULL | DocId, TxId newtypes with MVCC readiness |
| `memfuse-core/tx_buffer.rs` | WP-0.0 | ✅ FULL | Sharded TxBuffer with orphan reaper |
| `memfuse-core/snapshot.rs` | WP-0.0 | ✅ FULL | SnapshotId + SnapshotTracker for MVCC |
| `memfuse-core/traits.rs` | WP-0.0 | ✅ FULL | StorageEngine + VectorIndex traits |
| `memfuse-store/lsm.rs` | WP-1.1 | ✅ FULL | LSM engine with WAL integration |
| `memfuse-store/memtable.rs` | WP-1.1 | ✅ FULL | BTree-based MemTable |
| `memfuse-store/sstable.rs` | WP-1.1 | ✅ FULL | SSTable format with index block |
| `memfuse-store/wal.rs` | WP-1.1 | ✅ FULL | WAL with CRC32 checksums |
| `memfuse-store/compaction.rs` | WP-1.1 | ✅ FULL | Background compaction + tombstone GC |
| `memfuse-store/checkpoint.rs` | WP-5.1 (SAOS) | ⚠️ STUB | Skeleton only — no WAL replay logic yet |
| `memfuse-index/hnsw.rs` | WP-2.2 | ⚠️ PARTIAL | HNSW core complete, SQ8 quantization missing |
| `memfuse-index/distance.rs` | WP-2.2 | ✅ FULL | AVX2/AVX-512/portable-SIMD implementations |
| `memfuse-index/csr.rs` | WP-2.2 | ✅ FULL | CSR graph adjacency storage |
| `memfuse-db/lib.rs` | WP-1.2 | ⚠️ PARTIAL | Collection API exists, tests ignored, not fully wired |
| `memfuse-db/collection.rs` | WP-1.2 | ⚠️ PARTIAL | Collection struct defined, key-namespacing in progress |
| `memfuse-text/bm25.rs` | WP-2.1 | ⚠️ STUB | Basic BM25 scorer, no tests, no persistence |
| `memfuse-text/inverted.rs` | WP-2.1 | ⚠️ STUB | In-memory inverted index, no tests, no persistence |
| `memfuse-runtime/sandbox.rs` | — | ⚠️ STUB | Config + placeholder, no wasmtime integration |
| `memfuse-orchestrator/graph.rs` | — | ⚠️ STUB | StateGraph + AgentNode, no execution logic |
| `memfuse-py/lib.rs` | WP-3.1 | ⚠️ PARTIAL | PyO3 Agent/Collection, no tests, no full API |

## Coverage Summary

| State | Count |
|---|---|
| ✅ FULL | 11 modules |
| ⚠️ PARTIAL | 3 modules |
| ⚠️ STUB | 6 modules |
| ❌ MISSING | 0 modules (but 3 planned crates not yet created) |

## Specs Without Implementation

| Spec | Status |
|---|---|
| WP-3.2 Encryption at Rest | No `crypto.rs` exists |
| WP-4.1 Memory-Mapped I/O | No `mmap.rs` exists |
| WP-4.2 Advanced Filtering | No `filter.rs` exists |
| WP-4.3 DiskANN Out-of-Core | No `diskann.rs` exists |
