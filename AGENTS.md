# MemFuse — LLM Agent Context & Crate Directory

> **Phase:** Production Hardening & Feature Expansion  
> **Doctrine:** Zero-Panic / Sovereign Core / Triple-Test-Gate  
> **Updated:** 2026-05-23  
> **Tooling:** Gemini CLI + Google Jules (13-Agent Squad)

---

## Crate-Übersicht (11 Crates, ~10.8K LoC)

### Layer 3 — Interface (User-Facing)

| Crate | Rolle | LoC | Status |
|:------|:------|:----|:-------|
| **`memfuse-py`** | PyO3/NumPy Python-Bindings, shared Tokio Runtime | 528 | ✅ Stabil |

### Layer 2 — Orchestration

| Crate | Rolle | LoC | Status |
|:------|:------|:----|:-------|
| **`memfuse-db`** | Zentrale Facade: Collections, Hybrid-Search (RRF), Fusion, Namespace-Isolation | 1917 | ✅ Stabil |
| **`memfuse-checkpoint`** | Snapshot Registry, Time-Travel, MVCC-basiertes Checkpointing | 262 | 🟡 Scaffold |
| **`memfuse-orchestrator`** | Declarative StateGraph, Agent Workflow Engine | 89 | 🟡 Scaffold |
| **`memfuse-runtime`** | WASM Sandbox, Air-Gap Execution, Token-Budget | 163 | 🟡 Scaffold |

### Layer 1 — Sub-Engines (isoliert, keine gegenseitigen Imports)

| Crate | Rolle | LoC | Status |
|:------|:------|:----|:-------|
| **`memfuse-store`** | LSM-Tree: MemTables (MVCC), WAL, SSTables, Background Compaction | 2912 | ✅ Stabil |
| **`memfuse-index`** | HNSW-Graph (ANN), SIMD Distance, SQ8 Scalar Quantization | 2420 | ✅ Stabil |
| **`memfuse-text`** | BM25 Inverted Index, Tokenizer, German Morphology | 935 | ✅ Stabil |
| **`memfuse-graph`** | CSR-Graph für Entity-Relation Traversal (Signal 3) | 261 | 🟡 Scaffold |
| **`memfuse-crypto`** | AES-GCM Encryption at Rest, WAL Crypto | 216 | ✅ Stabil |

### Layer 0 — Kernel

| Crate | Rolle | LoC | Status |
|:------|:------|:----|:-------|
| **`memfuse-core`** | Shared Kernel: MemFuseError, Types, Traits, TxBuffer, Snapshots | 1129 | ✅ Stabil |

---

## Crate-Architektur (DAG)

```
                    ┌─────────────────────────────────┐
                    │        memfuse-py (L3)           │  Python API / pip install
                    └──────────────┬──────────────────┘
                                   │
          ┌────────────────────────┼────────────────────────┐
          │                        │                        │
┌─────────▼──────┐  ┌─────────────▼──────────┐  ┌─────────▼──────────┐
│ memfuse-       │  │ memfuse-               │  │ memfuse-           │
│ orchestrator   │  │ checkpoint             │  │ runtime            │
│ (L2/Scaffold)  │  │ (L2/Scaffold)          │  │ (L2/Scaffold)      │
└────────┬───────┘  └─────────────┬──────────┘  └─────────┬──────────┘
         │                        │                        │
         └────────────────────────┼────────────────────────┘
                                  │
                    ┌─────────────▼──────────────────┐
                    │      memfuse-db (L2)            │  Collections + Hybrid Search
                    │      Fusion + Namespaces        │
                    └──┬──────────┬──────────┬────────┘
                       │          │          │
          ┌────────────▼──┐ ┌────▼───────┐ ┌▼──────────────┐
          │ memfuse-store │ │ memfuse-   │ │ memfuse-text  │
          │ (L1)          │ │ index (L1) │ │ (L1)          │
          │ LSM+WAL+mmap  │ │ HNSW+SQ8   │ │ BM25+Morph    │
          └───────┬───────┘ └────┬───────┘ └───────┬───────┘
                  │              │                  │
                  │         ┌────▼───────┐          │
                  │         │ memfuse-   │          │
                  │         │ graph (L1) │          │
                  │         │ CSR        │          │
                  │         └────┬───────┘          │
                  │              │                  │
          ┌───────▼──────┐       │                  │
          │ memfuse-     │◄──────┘                  │
          │ crypto (L1)  │                          │
          └───────┬──────┘                          │
                  │                                 │
                  └──────────────┬──────────────────┘
                                 │
                    ┌────────────▼───────────────────┐
                    │  memfuse-core (L0 — Kernel)     │
                    │  MemFuseError · Types · Traits  │
                    │  DARF NICHTS IMPORTIEREN         │
                    └────────────────────────────────┘
```

### DAG-Invariante (Nicht verhandelbar)

- **Layer 0** importiert niemanden
- **Layer 1** importiert nur Layer 0 (+ `memfuse-crypto` für `memfuse-store`, `memfuse-graph` für `memfuse-index`)
- **Layer 2** importiert Layer 0 + Layer 1
- **Layer 3** importiert nur `memfuse-db` (via Facade)
- **Zyklische Abhängigkeiten** = CI-Breaker

---

## Per-Crate Kontext-Karten

### `memfuse-core` — Shared Kernel

| Key | Value |
|:----|:------|
| **Pfad** | `crates/memfuse-core/src/` |
| **Key Files** | `lib.rs`, `error.rs`, `traits.rs`, `types/`, `tx_buffer.rs`, `snapshot.rs` |
| **Public API** | `MemFuseError`, `Result<T>`, `DocId`, `TxId`, `ScoredDocument`, `StorageEngine` (trait), `VectorIndex` (trait), `TextIndex` (trait), `GraphIndex` (trait), `TxBuffer`, `Snapshot`, `WorkflowState`, `TokenBudget` |
| **Deps** | thiserror, serde, parking_lot, ahash, blake3, tokio, async-trait, tracing |
| **Invarianten** | `#![forbid(unsafe_code)]`, Zero external crate deps, alle Fehler über `MemFuseError` |

### `memfuse-store` — LSM Storage Engine

| Key | Value |
|:----|:------|
| **Pfad** | `crates/memfuse-store/src/` |
| **Key Files** | `lib.rs`, `lsm.rs`, `memtable.rs`, `sstable.rs`, `wal.rs`, `compaction.rs`, `cache.rs` |
| **Public API** | `LsmStorage`, `MemTable`, `Wal`, `SsTable`, `CompactionManager` |
| **Deps** | memfuse-core, memfuse-crypto, bytes, blake3, crc32fast, parking_lot, tokio, memmap2, lru |
| **Invarianten** | `#![forbid(unsafe_code)]`, nur `tokio::fs` (kein `std::fs`), MVCC via `seq_no` in MemTable, WAL-Recovery bei Startup |

### `memfuse-index` — Vector Engine

| Key | Value |
|:----|:------|
| **Pfad** | `crates/memfuse-index/src/` |
| **Key Files** | `lib.rs`, `hnsw.rs`, `distance.rs`, `quantize.rs`, `persistence.rs` |
| **Public API** | `HnswIndex`, `DistanceMetric`, `ScalarQuantizer` |
| **Deps** | memfuse-core, memfuse-graph, ahash, parking_lot, rand, roaring, tokio, memmap2 |
| **Invarianten** | `unsafe` NUR in `distance.rs` für SIMD (jeder Block mit `// SAFETY:` begründet), Graph-Layer via `memfuse-graph` |

### `memfuse-text` — Keyword Engine

| Key | Value |
|:----|:------|
| **Pfad** | `crates/memfuse-text/src/` |
| **Key Files** | `lib.rs`, `bm25.rs`, `inverted.rs`, `tokenizer.rs`, `morphology.rs` |
| **Public API** | `Bm25Scorer`, `InvertedIndex`, `BM25MorphIndex`, `DefaultTokenizer`, `GermanMorphTokenizer`, `GermanCompoundSplitter` |
| **Deps** | memfuse-core, unicode-segmentation, async-trait, bincode, serde, parking_lot |
| **Invarianten** | `#![forbid(unsafe_code)]`, Thread-safe via `parking_lot::RwLock` |

### `memfuse-db` — Orchestrator & Facade

| Key | Value |
|:----|:------|
| **Pfad** | `crates/memfuse-db/src/` |
| **Key Files** | `lib.rs`, `collection.rs`, `fusion.rs`, `context.rs`, `namespace.rs`, `migration.rs` |
| **Public API** | `MemFuse`, `MemFuseConfig`, `Collection`, `SearchResult`, `HybridSearchParams` |
| **Deps** | memfuse-core, memfuse-store, memfuse-index, memfuse-text, memfuse-checkpoint, serde_json, tokio |
| **Invarianten** | `#![forbid(unsafe_code)]`, einziger Crate der Store+Index+Text orchestriert, atomarer Commit über alle Sub-Engines |

### `memfuse-py` — Python Bindings

| Key | Value |
|:----|:------|
| **Pfad** | `crates/memfuse-py/src/lib.rs` (Single-File) |
| **Public API** | `PyMemFuse`, `PyCollection` (Python-seitig: `MemFuse`, `Collection`) |
| **Deps** | memfuse-db, pyo3, numpy, tokio, serde_json, pythonize |
| **Invarianten** | `#![forbid(unsafe_code)]`, shared `OnceLock<Runtime>`, alle Rust-Errors → `PyRuntimeError` |

### `memfuse-crypto` — Encryption at Rest

| Key | Value |
|:----|:------|
| **Pfad** | `crates/memfuse-crypto/src/` |
| **Key Files** | `lib.rs`, `crypto.rs`, `wal_crypto.rs` |
| **Public API** | `EncryptionKey`, `encrypt_block`, `decrypt_block`, `WalCryptoWriter` |
| **Deps** | memfuse-core, sha2, hmac, aes-gcm, hkdf |
| **Invarianten** | Key-Derivation via HKDF, AES-256-GCM für Block-Encryption |

### `memfuse-graph` — CSR Graph Engine

| Key | Value |
|:----|:------|
| **Pfad** | `crates/memfuse-graph/src/` |
| **Key Files** | `lib.rs`, `csr.rs` |
| **Public API** | `CsrGraph` (implements `GraphIndex` trait) |
| **Deps** | memfuse-core, parking_lot, ahash, tokio, async-trait |
| **Status** | 🟡 Scaffold — BFS traversal + score decay implementiert, noch keine Persistenz |

### `memfuse-checkpoint` — Time-Travel & Snapshots

| Key | Value |
|:----|:------|
| **Pfad** | `crates/memfuse-checkpoint/src/lib.rs` (Single-File) |
| **Public API** | `CheckpointRegistry`, `CheckpointMeta`, `PersistentCheckpointStore` |
| **Deps** | memfuse-core, tokio, serde, parking_lot |
| **Status** | 🟡 Scaffold — In-Memory MVCC Registry + persistent JSON Store |

### `memfuse-orchestrator` — StateGraph Engine

| Key | Value |
|:----|:------|
| **Pfad** | `crates/memfuse-orchestrator/src/` |
| **Key Files** | `lib.rs`, `graph.rs` |
| **Public API** | `GraphNode`, `WorkflowEdge`, `StateGraph` |
| **Deps** | memfuse-core |
| **Status** | 🟡 Scaffold — Deklarative Graph-Definiton, noch keine Execution |

### `memfuse-runtime` — WASM Sandbox

| Key | Value |
|:----|:------|
| **Pfad** | `crates/memfuse-runtime/src/` |
| **Key Files** | `lib.rs`, `sandbox.rs`, `airgap.rs` |
| **Public API** | `AgentRuntime` (trait), `WasmSandbox`, `AirGapProfile` |
| **Deps** | memfuse-core, async-trait |
| **Status** | 🟡 Scaffold — Trait-Definition + Stub-Impl, kein WASM-Runtime integriert |

---

## ⚠️ Sovereign Core Doctrine (ABSOLUT VERBINDLICH)

1. **`#![forbid(unsafe_code)]`** in jedem Crate (Ausnahme: [`distance.rs`](./crates/memfuse-index/src/distance.rs))
2. **Zero `.unwrap()`** außerhalb von `#[cfg(test)]` — nur `?` oder explizites Error-Handling
3. **Zero blockierendes I/O** in async-Kontexten — `tokio::fs` statt `std::fs`
4. **Warnings = Errors**: `cargo clippy -- -D warnings` muss immer sauber sein
5. **Jede neue public API** bekommt mindestens einen `#[tokio::test]` Contract-Test
6. **Jede Datei** braucht ein `//!` Crate/Module Doc-Comment im Header
7. **Backward Compatibility**: bestehende API-Signaturen dürfen nicht gebrochen werden

```rust
// ❌ VERBOTEN:
.unwrap()                    // → Result propagieren mit ?
std::fs::read()              // → tokio::fs verwenden
unsafe { ... }               // → Nur SIMD in distance.rs + // SAFETY: Beweis
```

---

## Triple-Test-Gate (DONE-Definition)

> **Ein Work Package gilt als DONE wenn und nur wenn:**
> 1. Alle zugehörigen Contract-Tests bestehen **3× hintereinander** ohne Änderung
> 2. `cargo clippy -- -D warnings` ist grün (0 Warnings)
> 3. Der GitHub Actions CI-Check ist grün (`.github/workflows/jules-quality-gate.yml`)
> 4. Keine bestehenden Tests des Workspace sind neu rot

```bash
just triple-test     # Triple-Test-Gate ausführen
just debt-audit      # Tech-Debt Scan (hat Priorität vor neuen Features)
just spec WP-X.Y-NAME  # Atomic Spec erstellen (Pflicht vor Implementierung)
```

---

## Work Package Status (Stand: 2026-05-23)

### Phase 1 — Foundation

| WP | Name | Crate(s) | Status | Spec |
|---|---|---|---|---|
| **WP-0.0** | Dependency Audit & Tech Debt | alle | ✅ Stabil | [SPEC](./docs/specs/SPEC-20260505-WP-0.0-DependencyAudit_done.md) |
| **WP-1.1** | Background Compaction | `memfuse-store` | ✅ Stabil | [SPEC](./docs/specs/SPEC-20260505-WP-1.1-Compaction_done.md) |
| **WP-1.2** | Collections / Namespaces | `memfuse-db` | ✅ Stabil | [SPEC](./docs/specs/SPEC-20260505-WP-1.2-Collections_done.md) |
| **WP-1.3** | Atomic Commit | `memfuse-db` | ✅ Stabil | [SPEC](./docs/specs/SPEC-20260509-WP-1.3-AtomicCommit_done.md) |

### Phase 2 — Search & Retrieval

| WP | Name | Crate(s) | Status | Spec |
|---|---|---|---|---|
| **WP-2.1** | Hybrid Search (BM25+RRF) | `memfuse-text`, `memfuse-db` | ✅ Stabil | [SPEC](./docs/specs/SPEC-20260505-WP-2.1-HybridSearch_done.md) |
| **WP-2.2** | Scalar Quantization (SQ8) | `memfuse-index` | ✅ Stabil | [SPEC](./docs/specs/SPEC-20260505-WP-2.2-Quantization_done.md) |

### Phase 3 — User Interface & Security

| WP | Name | Crate(s) | Status | Spec |
|---|---|---|---|---|
| **WP-3.1** | Python Bindings (PyO3) | `memfuse-py` | ✅ Stabil | [SPEC](./docs/specs/SPEC-20260505-WP-3.1-PythonBindings_done.md) |
| **WP-3.2** | Encryption at Rest | `memfuse-crypto` | ✅ Stabil | [SPEC](./docs/specs/SPEC-20260505-WP-3.2-Encryption_done.md) |

### Phase 4 — Hyper-Scale

| WP | Name | Crate(s) | Status | Spec |
|---|---|---|---|---|
| **WP-4.1** | Memory-Mapped I/O | `memfuse-store` | ✅ Stabil | [SPEC](./docs/specs/SPEC-20260505-WP-4.x-Scale.md) |
| **WP-4.2** | Advanced Filtering | `memfuse-db` | ✅ Stabil | [SPEC](./docs/specs/SPEC-20260505-WP-4.x-Scale.md) |
| **WP-4.3** | DiskANN Out-of-Core | `memfuse-index` | 🟡 Refactor | [SPEC](./docs/specs/SPEC-20260505-WP-4.x-Scale.md) |

### Phase 5 — SAOS (Agent OS)

| WP | Name | Crate(s) | Status | Spec |
|---|---|---|---|---|
| **WP-5.1** | Checkpointing / Time-Travel | `memfuse-checkpoint` | 🟡 Scaffold | [SPEC](./docs/specs/SPEC-20260508-WP-5.1-Checkpointing.md) |
| **WP-5.2** | WASM Sandbox | `memfuse-runtime` | 🟡 Scaffold | [SPEC](./docs/specs/SPEC-SAOS-WP-5.2-WasmSandbox.md) |
| **WP-5.3** | Agent Orchestration | `memfuse-orchestrator` | 🟡 Scaffold | [SPEC](./docs/specs/SPEC-SAOS-WP-5.3-AgentOrchestration.md) |

### Phase 6 — Goldstandard (Zukunft)

| WP | Name | Crate(s) | Status | Spec |
|---|---|---|---|---|
| **WP-6.1** | 4-Signal Fusion API | `memfuse-db`, `memfuse-graph` | ⬜ Offen | [SPEC](./docs/specs/SPEC-20260509-GOLDSTANDARD-Funktionskatalog.md) |
| **WP-6.2** | Declarative StateGraph API | `memfuse-orchestrator` | ⬜ Offen | [SPEC](./docs/specs/SPEC-20260509-GOLDSTANDARD-Funktionskatalog.md) |
| **WP-6.3** | Autonomes Kontext-Management | `memfuse-db` | ⬜ Offen | [SPEC](./docs/specs/SPEC-20260509-GOLDSTANDARD-Funktionskatalog.md) |
| **WP-6.4** | Multi-Agent Namespaces | `memfuse-db` | ⬜ Offen | [SPEC](./docs/specs/SPEC-20260509-GOLDSTANDARD-Funktionskatalog.md) |
| **WP-6.5** | Morphologische Inferenz-Optimierung | `memfuse-text` | ⬜ Offen | [SPEC](./docs/specs/SPEC-20260509-GOLDSTANDARD-Funktionskatalog.md) |
| **WP-6.6** | Air-Gap Deployment Profile | `memfuse-runtime` | ⬜ Offen | [SPEC](./docs/specs/SPEC-20260509-GOLDSTANDARD-Funktionskatalog.md) |
| **WP-6.7** | Kryptografische WAL-Verifikation | `memfuse-crypto` | ⬜ Offen | [SPEC](./docs/specs/SPEC-20260509-GOLDSTANDARD-Funktionskatalog.md) |

### Phase 7 — RAG & Connectivity (NEU)

| WP | Name | Crate(s) | Status | Spec |
|---|---|---|---|---|
| **WP-7.1** | Markdown Chunker | `memfuse-db` | ⬜ Offen | [SPEC](./docs/specs/SPEC-20260524-WP-7.1-MarkdownChunker.md) |
| **WP-7.2** | HNSW Persistence | `memfuse-index` | ⬜ Offen | [SPEC](./docs/specs/SPEC-20260524-WP-7.2-HnswPersistence.md) |
| **WP-7.3** | MCP Provider | `memfuse-py` | ⬜ Offen | [SPEC](./docs/specs/SPEC-20260524-WP-7.3-MCPProvider.md) |

---

## Bekannte Offene Audit-Findings (Stand: 2026-05-24)

| ID | Severity | Crate | Beschreibung |
|---|---|---|---|
| CRIT-001 | 🔴 | `memfuse-core` | `DocId::from_key()` nutzt `.expect()` — Zero-Panic Verstoß |
| HIGH-001 | 🟠 | `memfuse-store` | WAL-Einträge werden bei Replay nicht CRC-verifiziert |
| HIGH-002 | 🟠 | `memfuse-checkpoint` | Persistent Store hat keinen Locking-Mechanismus |

> Vollständige Audit-Reports: [`docs/audit/`](./docs/audit/)

---

## Autonomous Squad Protocol (13 Jules Agents)

| # | Role | Domain | Schedule |
|---|---|---|---|
| 13 | **Debt Hunter** | Tech Debt & Invariant Cleanup | 05:00 UTC |
| 01 | **Core Guardian** | `memfuse-core` & Shared Types | 06:00 UTC |
| 02 | **Store Engineer** | `memfuse-store` (LSM / WAL) | 07:00 UTC |
| 03 | **Index Master** | `memfuse-index` (HNSW / SQ8) | 08:00 UTC |
| 04 | **Collection Architect** | `memfuse-db` (Collections / Fusion) | 09:00 UTC |
| 05 | **Text Analyst** | `memfuse-text` (BM25 / Morphology) | 10:00 UTC |
| 06 | **Python Bridge** | `memfuse-py` (PyO3 Bindings) | 11:00 UTC |
| 07 | **QA Cross-Crate** | Integration & PR Verification | 20:00 UTC |
| 08 | **Runtime Architect** | `memfuse-runtime` (WASM Sandbox) | 12:00 UTC |
| 09 | **Orchestration Lead** | `memfuse-orchestrator` (StateGraph) | 13:00 UTC |
| 10 | **Security Engineer** | `memfuse-crypto` (Encryption) | 14:00 UTC |
| 11 | **Graph Engineer** | `memfuse-graph` (CSR / 4-Signal) | 15:00 UTC |
| 12 | **Checkpoint Lead** | `memfuse-checkpoint` (Time-Travel) | 16:00 UTC |

**Dynamic Queue Dispatch:** Bei Push auf `develop` berechnet der `jules-queue-dispatcher` den nächsten Agent in der logischen Abhängigkeitskette. Lock-Sync via `jules-sync-locks.sh` blockiert high-level Tasks solange low-level Crates `WIP` sind.

---

## Tooling & Verzeichnis-Referenzen

| Kontext | Pfad |
|---------|------|
| Architektur-Dokument | [`.agent/context/ARCHITECTURE.md`](./.agent/context/ARCHITECTURE.md) |
| Atomic Specs | [`docs/specs/SPEC-*.md`](./docs/specs/) |
| Audit-Reports | [`docs/audit/`](./docs/audit/) |
| Jules-Prompts | `.agent/jules/prompts/accounts/XX-name.md` |
| Jules-Schedule | `.agent/jules/SCHEDULE.md` |
| CI-Pipeline | `.github/workflows/jules-quality-gate.yml` |
| Gemini Skills | `.gemini/skills/` |
| Agent Workflows | `.agent/workflows/` |
