# FORENSIC INVENTORY - MEMFUSE

## Workspace Overview
**Total Crates:** 11
**Total LoC:** ~26.2K (Tokei)
**Rust Version:** 1.80+ (Workspace requirement)

## Crate Inventory

### 1. memfuse-core
- **Traits:** [Checkpoint](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-checkpoint/src/lib.rs#24-32), `Snapshot`, `StorageEngine`, `VectorIndex`, `TextIndex`, `GraphIndex`
- **Critical Structs:** `TxBuffer`, `ResourceTracker`, `TxId`, `DocId`, `MemFuseError`
- **Status:** Stable base.

### 2. memfuse-store (LSM)
- **Status:** Advanced. Implements WAL with HMAC chaining and AES-GCM encryption.
- **Skeletons:** Compaction loop identified as `tokio::spawn` but needs validation on resource limits.

### 3. memfuse-index (HNSW + SIMD)
- **Status:** Functional. Supports SQ8 quantization and dynamic rebuilds (>20% deletes).
- **Critical:** Uses `unsafe_code` for SIMD/Mmap performance. Hand-rolled distance metrics.

### 4. memfuse-text
- **Status:** Implements inversion index and BM25.
- **Architecture Note:** Potential DAG violation noted in comments (reimporting `memfuse-store`), verified as `dev-dependency`.

### 5. memfuse-crypto
- **Status:** Key management and WAL crypto. Uses `aes-gcm`.

### 6. memfuse-graph
- **Status:** CSR-Graph implementation.

### 7. memfuse-db (Orchestrator)
- **Status:** Manages namespaces and hybrid fusion.

### 8. memfuse-py
- **Status:** PyO3 Bindings.

### 9. memfuse-sandbox (WASM)
- **Status:** Execution environment for tools.

### 10. memfuse-checkpoint
- **Status:** **CRITICAL**. [create_checkpoint](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-checkpoint/src/lib.rs#101-150) signature mismatch in tests. [commit](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-checkpoint/src/lib.rs#275-278), [rollback](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-checkpoint/src/lib.rs#475-478), [flush](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-checkpoint/src/lib.rs#284-287) are partial placeholders in some contexts.

### 11. memfuse-saos-agent
- **Status:** Orchestration engine. [persist_final_state](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-saos-agent/src/engine.rs#194-210) is implemented (contrary to previous audit reports).

## Skeletons & TODOs Scavenge
- `todo!()` found in:
    - Documentation mentions mostly.
- [E()](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/wal.rs#34-44) macros: Zero found (Audit success).
- `unwrap()` in production code:
    - `crates/memfuse-index/src/hnsw.rs:503` (Unaligned F32 read during mmap load)
    - [crates/memfuse-store/src/sstable.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/sstable.rs) (needs review)

## Dependency Map
- **Core Dependencies:** `tokio`, `serde`, `parking_lot`, `ahash`, `roaring`.
- **Crypto:** `blake3`, `aes-gcm`.
- **WASM:** `wasmtime`.
