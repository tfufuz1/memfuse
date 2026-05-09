# MemFuse — Algorithmic Topology (Audit v2)
**Stand:** 2026-05-08T23:30 UTC+2  
**Auditor:** Elite Algorithmic Architect (v2 — re-audit after fixes)

## Crate Inventory

| Crate | Rolle | LoC | Status |
|-------|-------|-----|--------|
| `memfuse-core` | Traits, Types, TxBuffer, SnapshotRegistry | ~520 | ✅ Stabil |
| `memfuse-store` | LSM, MemTable, SSTable, WAL, Compaction, Checkpoint | ~2520 | ✅ Stabil |
| `memfuse-index` | HNSW, SIMD Distance, CSR Graph | ~1524 | ✅ Stabil |
| `memfuse-db` | Orchestrator, Collections, Facade | ~984 | ✅ Stabil |
| `memfuse-text` | BM25, Inverted Index | ~132 | ✅ Stabil |
| `memfuse-orchestrator` | Graph abstraction (SAOS) | ~exists | 🔵 WP |
| `memfuse-runtime` | Sandbox runtime (SAOS) | ~exists | 🔵 WP |
| `memfuse-checkpoint` | Point-in-time recovery | ~exists | 🔵 WP |
| `memfuse-py` | Python bindings (PyO3) | ~exists | 🔵 WP |

## Top-10 Files by Complexity

| File | LoC | Domain |
|------|-----|--------|
| `memfuse-index/src/hnsw.rs` | 929 | HNSW Graph |
| `memfuse-store/src/sstable.rs` | 778 | SSTable I/O |
| `memfuse-store/src/lsm.rs` | 698 | LSM Orchestrator |
| `memfuse-db/src/lib.rs` | 692 | DB Facade |
| `memfuse-store/src/compaction.rs` | 635 | Compaction |
| `memfuse-index/src/distance.rs` | 506 | SIMD Distance |
| `memfuse-db/src/collection.rs` | 292 | Collections |
| `memfuse-store/src/wal.rs` | 286 | WAL |
| `memfuse-core/src/types.rs` | 284 | Core Types |
| `memfuse-core/src/tx_buffer.rs` | ~180 | TxBuffer |

## Synchronization Primitives Registry

| Primitive | File | Line(s) | Purpose |
|-----------|------|---------|---------|
| `tokio::sync::RwLock` | `lsm.rs` | L63 | LsmState (memtable, WAL) |
| `tokio::sync::RwLock` | `lsm.rs` | L65 | SSTable list |
| `tokio::sync::Mutex` | `lsm.rs` | L72 | Commit serialization |
| `tokio::sync::Mutex` | `wal.rs` | L99 | WAL file access |
| `parking_lot::RwLock` | `memtable.rs` | L19 | MemTable entries |
| `parking_lot::RwLock` | `hnsw.rs` | L129 | HNSW nodes + doc_to_node |
| `parking_lot::RwLock` | `hnsw.rs` | L131 | Entry point |
| `parking_lot::RwLock` | `hnsw.rs` | L135 | Deleted nodes bitmap |
| `parking_lot::RwLock` | `sstable.rs` | L24 | Block cache (LRU) |
| `tokio::sync::Mutex` | `hnsw.rs` | L138 | Write serialization |
| `parking_lot::Mutex` | `snapshot.rs` | L20 | Snapshot registry |
| `AtomicU64` | `lsm.rs` | L70 | seq_no counter (SeqCst) |
| `AtomicU64` | `hnsw.rs` | L132 | max_layer (SeqCst) |
| `AtomicU64` | `hnsw.rs` | L136 | deleted_count (SeqCst) |
| `AtomicBool` | `hnsw.rs` | L137 | rebuilding flag (SeqCst) |
| `AtomicU64` | `snapshot.rs` | L21 | min_active_seqno (Acq/Rel) |
| `AtomicUsize` | `memtable.rs` | L20 | Size tracking (Relaxed) |
