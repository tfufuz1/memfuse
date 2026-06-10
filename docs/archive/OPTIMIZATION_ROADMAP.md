# MemFuse Optimization Roadmap

Based on the forensic audit and subsequent remediations, the following roadmap tracks the stabilization and scaling of the MemFuse architecture.

## Phase 1: Security & DAG Integrity (TIER 1) - ✅ COMPLETED
1. **DAG Architecture Enforcement**: `memfuse-text` cycle broken.
2. **Cryptographic Hardening**: HKDF key derivation and AtomicU64 nonces implemented.
3. **Storage Safe Rust**: `memfuse-store` is 100% safe Rust.

## Phase 2: Feature Completion & Orchestration - ✅ COMPLETED
1. **Unified Database Logic**: `memfuse-db` collection management (`COL-001/002/003`) and hybrid search (`SEARCH-001`) implemented.
2. **Storage Durability**: CompactionEngine and snapshot pinning/unpinning functional.
3. **Sandbox Verification**: WASM host functions and AirGapVerifier implemented.

## Phase 3: Performance Tuning & Stabilization - ✅ COMPLETED
1. **DiskANN Async I/O**: Blocking Mmap calls wrapped in `spawn_blocking`.
2. **OpenTelemetry Integration**: Tracing expanded across all hot paths.
3. **Skeleton Remediation**: Python MCP zero-vector spoofing and HNSW builder resource caps fixed.

---

## Phase 4: Production Scale & High Availability (NEXT)

Zur finalen Vollendung des Endprodukts ("Vollfunktionsfähig") müssen horizontale Skalierung, API-Ergonomie und Zero-Copy IPC umgesetzt werden.

### 1. Raft-based Replication (`memfuse-cluster`)
- **Architektur:** Implementierung einer neuen Crate `memfuse-cluster` für das Raft-Konsensprotokoll (via `openraft`).
- **Aufgabe (REP-001):** Anbindung des `memfuse-store` WALs an die Raft State Machine.
- **Aufgabe (REP-002):** Leader-Election Algorithmus einbinden. Alle Writes gehen an den Leader (Strong Consistency), Reads können an Follower delegiert werden (Eventual Consistency).

### 2. Auto-Embedding Service (`memfuse-embed`)
- **Architektur:** Native, In-Engine Embedding-Generierung für nahtlose Developer Experience (DX).
- **Aufgabe (EMB-001):** Integration von `ort` (ONNX Runtime) um lokale Embedding-Modelle (z.B. all-MiniLM-L6-v2) asynchron auszuführen.
- **Aufgabe (EMB-002):** Anpassung der `memfuse-py`-Bindings, damit Vektoren bei `memfuse_search` und `memfuse_insert` optional sind und vom Background-Worker automatisch aus dem `text` generiert werden.

### 3. Zero-Copy Serialization (IPC / FlatBuffers)
- **Architektur:** Vermeidung von CPU-Overhead bei Deserialisierung auf High-Throughput-Pfaden.
- **Aufgabe (IPC-001):** Definition eines FlatBuffers-Schemas (`.fbs`) für `ScoredDocument`, `InsertRequest` und Datensätze.
- **Aufgabe (IPC-002):** Refactoring von `memfuse-sandbox` zur Übergabe von Memory-Pointern (Zero-Copy) anstatt teurer Allocation-Kopien (JSON).

### 4. SIMD Quantization Kernels (`memfuse-index`)
- **Architektur:** Hardware-beschleunigtes Brute-Force Scanning für den Quantized Layer.
- **Aufgabe (SIMD-001):** Ausweitung von `distance.rs` auf dedizierte `u8` AVX-512 Dot-Product und Euclidean Distance Macros.
- **Aufgabe (SIMD-002):** Dynamisches Feature-Flag Dispatching (`target_feature = "avx512bw"`).


