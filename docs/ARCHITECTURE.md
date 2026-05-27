# MemFuse — Architecture Source of Truth

This document defines the structural invariants and component responsibilities of the MemFuse system.

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
- **`memfuse-core`**: Global types (`DocId`, `TxId`), shared traits (`StorageEngine`, `VectorIndex`), and unified error handling (`MemFuseError`). **Invariant: No I/O, no async (target).**

### Layer 1: The Engines
- **`memfuse-crypto`**: AES-256-GCM encryption and HKDF key derivation.
- **`memfuse-store`**: Persistent LSM-Tree storage. Manages WAL, MemTables, and SSTables.
- **`memfuse-index`**: HNSW vector index with SIMD-accelerated distance metrics.
- **`memfuse-text`**: BM25 inverted index and morphological tokenizers.
- **`memfuse-graph`**: CSR-Graph for relationship traversal (Signal 3).

### Layer 2: Orchestration
- **`memfuse-db`**: The main facade. Orchestrates Engines to provide Hybrid Search (4-Signal Fusion), multi-tenancy (Namespaces), and atomic transactions.

### Layer 3: Bindings
- **`memfuse-py`**: Python bridge using PyO3 and NumPy.

---

## 🛡️ Critical Invariants

1.  **Sovereign Core Doctrine**:
    - `#![forbid(unsafe_code)]` in all crates except `memfuse-index` (specifically marked SIMD zones).
    - Zero-Panic: No `.unwrap()` or `.expect()` in production code.
2.  **LSM Write Guarantee**: WAL write **must** be flushed and synced before MemTable modification.
3.  **Isolation**: Namespaces must be physically prefixed in storage to prevent leakage.
4.  **Resource Control**: All memory-intensive operations must register with the `ResourceTracker`.

---

## 🔗 Referenced ADRs (Architectural Decision Records)

- **ADR-001**: LSM-Tree for persistence (Storage choice).
- **ADR-002**: HNSW for vector indexing.
- **ADR-003**: RRF (Reciprocal Rank Fusion) for Signal Hybridization.
- **ADR-004**: Sovereign Core (Safety & Security policy).
