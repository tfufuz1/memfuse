# MemFuse — Architecture Source of Truth

This document defines the structural invariants and component responsibilities of the MemFuse system. **Current Status: TIER 1-3 Stabilized.**

---

## 🏗️ Layered Architecture (The DAG)

MemFuse follows a strict Directed Acyclic Graph (DAG) for dependencies. Circular dependencies are strictly forbidden.

```mermaid
graph TD
    L0[Layer 0: Core] --> L1
    L1[Layer 1: Engines] --> L2
    L2[Layer 2: Orchestration] --> L3
    L2 --> FROZEN[Frozen Features]

    subgraph L0
        core[memfuse-core]
    end

    subgraph L1
        crypto[memfuse-crypto]
        graph[memfuse-graph]
        store[memfuse-store]
        index[memfuse-index]
        text[memfuse-text]
    end

    subgraph L2
        db[memfuse-db]
    end

    subgraph L3
        py[memfuse-py]
    end

    subgraph FROZEN
        checkpoint[memfuse-checkpoint]
        saos[memfuse-saos-agent]
        sandbox[memfuse-sandbox]
    end

    crypto --> store
    graph --> index
    store --> db
    index --> db
    text --> db
    db --> py
```

---

## 📦 Crate Responsibilities

### Layer 0: The Foundation
- **`memfuse-core`**: Global types (`DocId`, `TxId`), shared traits (`StorageEngine`, `VectorIndex`), and unified error handling (`MemFuseError`).

### Layer 1: The Engines
- **`memfuse-crypto`**: AES-256-GCM encryption and HKDF key derivation. Verified non-reusable nonces.
- **`memfuse-store`**: Persistent LSM-Tree storage. Manages WAL, MemTables, and SSTables. **Safe Rust enforced.**
- **`memfuse-index`**: HNSW and DiskANN vector indices. Uses `HnswConfigBuilder` for resource limit enforcement. Async-safe I/O via `spawn_blocking`.
- **`memfuse-text`**: BM25 inverted index and morphological tokenizers.
- **`memfuse-graph`**: CSR-Graph for relationship traversal.

### Layer 2: Orchestration
- **`memfuse-db`**: The main facade. Orchestrates Engines to provide Hybrid Search (4-Signal Fusion). Atomic transactions. **Zero-Panic enforced.**

### Layer 3: Bindings
- **`memfuse-py`**: Python bridge using PyO3. Strict exception mapping and vector validation.

---

## 🛡️ Critical Invariants

1.  **Sovereign Core Doctrine**:
    - `#![forbid(unsafe_code)]` in all crates except `memfuse-index` (SIMD).
    - Zero-Panic: No `.unwrap()` or `.expect()` (verified via audits).
2.  **LSM Write Guarantee**: WAL write **must** be flushed and synced before MemTable modification.
3.  **Resource Control**: `HnswConfigBuilder` enforces hard limits (e.g., 50M records) to prevent heap-bombing OOM.
4.  **Cryptographic Isolation**: Unique sub-key derivation per file via HKDF context.


---

## 🔗 Referenced ADRs (Architectural Decision Records)

- **ADR-001**: LSM-Tree for persistence (Storage choice).
- **ADR-002**: HNSW for vector indexing.
- **ADR-003**: RRF (Reciprocal Rank Fusion) for Signal Hybridization.
- **ADR-004**: Sovereign Core (Safety & Security policy).
