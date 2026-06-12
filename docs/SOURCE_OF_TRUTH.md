# MemFuse — Source of Truth (SOT)

> Dieses Dokument agiert als fester Bestandteil des **Unified Documentation Systems** (siehe `CONSTITUTION.md`) und ist das einzige **Living State Document** für Architektur, Crate-Status, offene Findings, und die Implementierungs-Roadmap. Es gibt keine persistenten Specs oder Archiv-Dokumente – jegliches Wissen wird hier konsolidiert.

---

## 1. Architektur

### 1.1 Schichtmodell (DAG)

```mermaid
graph TD
    subgraph "Layer 0 — Foundation"
        core["memfuse-core<br/>Types, Traits, Errors"]
    end

    subgraph "Layer 1 — Engines"
        crypto["memfuse-crypto"]
        store["memfuse-store"]
        index["memfuse-index"]
        text["memfuse-text"]
        graph["memfuse-graph"]
    end

    subgraph "Layer 2 — Orchestration"
        db["memfuse-db"]
    end

    subgraph "Layer 3 — Bindings"
        py["memfuse-py"]
    end

    subgraph "Frozen (Feature-Complete)"
        ckpt["memfuse-checkpoint"]
        saos["memfuse-saos-agent"]
        sandbox["memfuse-sandbox"]
    end

    core --> crypto
    core --> store
    core --> index
    core --> text
    core --> graph
    crypto --> store
    graph --> index
    store --> db
    index --> db
    text --> db
    sandbox --> db
    ckpt --> db
    db --> py
    db --> saos
```

### 1.2 Kritische Invarianten

| # | Invariante | Enforcement |
|---|---|---|
| 1 | **Sovereign Core Doctrine** | `#![forbid(unsafe_code)]` in allen Crates außer `memfuse-index` (`#![deny(unsafe_code)]`) |
| 2 | **Zero-Panic** | Kein `.unwrap()`/`.expect()` im Produktionscode. `?`-Operator mit `MemFuseError` |
| 3 | **WAL-First** | Kein MemTable-Write ohne vorherigen WAL-Flush + `sync_all()` |
| 4 | **Resource Control** | `HnswConfigBuilder` erzwingt Hard-Limits (max 50M Records) |
| 5 | **Kryptographische Isolation** | HKDF Sub-Key Derivation pro Datei, AtomicU64 Monotonic Nonces |
| 6 | **DAG Integrity** | Keine zirkulären Crate-Abhängigkeiten. Layer N darf nur Layer < N importieren |

### 1.3 ADRs (Architectural Decision Records)

| ADR | Entscheidung |
|---|---|
| ADR-001 | LSM-Tree für Persistenz |
| ADR-002 | HNSW für Vektor-Indexierung |
| ADR-003 | RRF (Reciprocal Rank Fusion) für 4-Signal Hybridisierung |
| ADR-004 | Sovereign Core (Safety & Security Policy) |

---

## 2. Crate-Inventar

### 2.1 Übersicht

| Crate | Layer | LOC | Tests | Status | Verantwortung |
|---|---|---|---|---|---|
| `memfuse-core` | 0 | 1.126 | 20 | 🟢 Clean | Globale Types (`DocId`, `TxId`), Traits (`StorageEngine`, `VectorIndex`), `MemFuseError` |
| `memfuse-store` | 1 | 4.130 | 43 | 🟡 Minor | LSM-Tree: WAL, MemTable, SSTable, Compaction, Checkpointing |
| `memfuse-index` | 1 | 3.503 | 26 | 🟢 Clean | HNSW, DiskANN, SQ8 Quantization, SIMD Distance Functions |
| `memfuse-text` | 1 | 962 | 20 | 🟢 Clean | BM25 Inverted Index, Morphologische Tokenizer |
| `memfuse-crypto` | 1 | 313 | 13 | 🟢 Clean | AES-256-GCM, HKDF Key Derivation, HMAC-WAL Integrity |
| `memfuse-graph` | 1 | 521 | 8 | 🟢 Clean | CSR-Graph für Relationship Traversal |
| `memfuse-db` | 2 | 2.456 | 49 | 🟡 Minor | Orchestration: Hybrid Search, Collections, Transactions, Markdown Chunker |
| `memfuse-py` | 3 | 536 | 0* | 🟢 Clean | PyO3 Bindings, MCP Provider, Vector Validation |
| `memfuse-checkpoint` | Frozen | 317 | 4 | 🟢 Clean | MVCC Snapshots, Backup Verification |
| `memfuse-saos-agent` | Frozen | 419 | 15 | 🟢 Clean | Deterministischer Graph-Resolver, Token Budget Tracking |
| `memfuse-sandbox` | Frozen | 470 | 15 | 🟢 Clean | Wasmtime Sandbox, AirGapVerifier, Host Functions |
| **Gesamt** | | **14.753** | **213** | | |

\* `memfuse-py` Tests erfordern Python-Runtime und werden via `maturin develop` separat ausgeführt.

### 2.2 Detaillierter Crate-Status

#### memfuse-core (Layer 0)

**Traits:**
- `StorageEngine` — Async KV-Store mit Transaktionen (put/get/delete/commit/rollback)
- `VectorIndex` — Vektor-Insert/Search/Delete mit Transaktionen
- `TextIndex` — Invertierter Index (index_document/search/stats)
- `GraphIndex` — Graph-Traversal mit Transaktionen
- `Checkpoint` / `Snapshot` — Persistenz-Snapshots

**Types:** `DocId`, `TxId`, `EntityId`, `Embedding`, `ScoredDocument`, `MemFuseError`, `FusionWeights`, `HybridQuery`, `ResourceBudget`, `TxBuffer`

**Status:** ✅ Zero Skeletons, Zero `unsafe`, Zero Production-`unwrap()`

---

#### memfuse-store (Layer 1)

**Komponenten:**
- `Wal` — Write-Ahead-Log mit HMAC-Chaining, Replay, Truncation
- `MemTable` — In-Memory sorted KV-Store (BTreeMap)
- `SstableBuilder`/`SstableReader` — Immutable on-disk sorted files mit Bloom-Filter, Block-Cache, Encryption
- `CompactionEngine` — Tiered Compaction mit `yield_now()`, Cancellation-Token, budgetiertem I/O
- `LsmStorage` — Orchestriert WAL → MemTable → SSTable Pipeline
- `Checkpointer` — Atomic State Snapshots
- `MmapReader` — Memory-Mapped File Access

**Offene Findings:**

| Finding | Beschreibung | Priorität |
|---|---|---|
| **FIND-STO-001** | WAL CRC-Checksum fehlt. Korrupte Entries werden bei Replay nicht erkannt. | TIER 2 |

---

#### memfuse-index (Layer 1)

**Komponenten:**
- `HnswIndex` / `HnswIndexCore` — Hierarchical Navigable Small World Graph mit Transaktionen, Rebuild, Delete-Tracking
- `HnswConfig` + Builder — Resource-Capped Konfiguration (max 50M, max 4096 Dims)
- `DiskAnnIndex` — Disk-basierter ANN mit Mmap, `spawn_blocking` für Async-Safety
- `ScalarQuantizer` — SQ8 Quantization (f32 → u8)
- `distance.rs` — AVX-512/AVX2 SIMD Kernels + Scalar Fallback, Cosine/Euclidean/Dot
- `persistence.rs` — HNSW Binary Save/Load Format mit `MmapIndex`

**Status:** ✅ Alle Findings gelöst (NaN-Poisoning, Rebuild Threshold, Async I/O)

**Hinweis:** `distance.rs` enthält 42 `unsafe` Blöcke für SIMD-Intrinsics. Alle haben `// SAFETY:` Kommentare. `#![deny(unsafe_op_in_unsafe_fn)]` ist gesetzt.

---

#### memfuse-text (Layer 1)

**Komponenten:**
- `InvertedIndex<S>` — BM25 Scoring mit generischem StorageEngine Backend
- `BM25MorphIndex<S>` — Morphologischer Index mit Compound-Splitting
- `Bm25Scorer<S>` — Standalone BM25 Scorer
- `GermanCompoundSplitter` / `GermanMorphTokenizer` — Deutsch-spezifische Textverarbeitung
- `DefaultTokenizer` / `PassthroughTokenizer` — Standard-Tokenizer

**Status:** ✅ DAG-Violation gelöst, BM25 Division-by-Zero gefixt

---

#### memfuse-crypto (Layer 1)

**Komponenten:**
- `KeyManager` — AES-256-GCM mit HKDF Key Derivation, Random Salt Generation, AtomicU64 Nonce Counter
- `EncryptedWal` — Transparente WAL-Verschlüsselung
- `WalHmac` / `IntegrityVerifier` — HMAC-SHA256 Append-Only Integrity Chain

**Status:** ✅ Nonce-Reuse Fix, Dynamic Salt, Per-File Sub-Key Isolation

---

#### memfuse-graph (Layer 1)

**Komponenten:**
- `CsrGraph` — Compressed Sparse Row Graph mit Transaktionen, BFS Traversal, Transaction Isolation, Compaction

**Status:** ✅ Transaction Isolation gefixt (nur committed Edges in Compaction)

---

#### memfuse-db (Layer 2)

**Komponenten:**
- `MemFuse` — Hauptfassade: Open, Insert, Search, Get, Update, Delete, Hybrid Search
- `Collection<S>` — Benannte Collections mit eigener Storage/Index/Text-Engine Instanz
- `DbTransaction` — ACID-Transaktionen über Storage+Index+Text
- `ContextManager` / `SpatialFence` — Kontextfenster-Management
- `NamespaceRegistry` — Multi-Tenant Namespace Isolation
- `MarkdownChunker` — Hierarchisches Markdown-Chunking
- `fusion.rs` — 4-Signal RRF Fusion (Vector + Text + Graph + Recency)
- `filter.rs` — Metadata-Filter Engine (Eq, Gt, Lt, In, And, Or, Not)
- `reaper.rs` — Background Cleanup Tasks

**Offene Findings:**

| Finding | Beschreibung | Priorität |
|---|---|---|
| **FIND-DB-002** | OpenTelemetry Tracing Coverage unvollständig. Basis-`tracing::instrument` fehlt auf vielen pub-Methoden. | TIER 3 |

---

#### memfuse-py (Layer 3)

**Komponenten:**
- `PyMemFuse` — Python-Klasse für MemFuse-Instanz (open, insert, search, get, stats)
- `PyCollection` — Python-Klasse für Collection-Zugriff
- `PySearchResult`, `PyDocument`, `PyDbStats` — Python-Wrappers
- MCP Provider mit Zero-Vector Spoofing Detection

**Status:** ✅ Exception Mapping implementiert (ValueError, IOError, RuntimeError)

---

#### memfuse-checkpoint (Frozen)

- MVCC Checkpoint Store mit Load/Save/List
- **Status:** ✅ Feature-Complete

#### memfuse-saos-agent (Frozen)

- Deterministic Graph Resolution (4-Signal Fusion)
- Token Budget Tracking, Node Execution
- **Status:** ✅ Feature-Complete

#### memfuse-sandbox (Frozen)

- Wasmtime-basierte WASM Sandbox
- AirGapVerifier (Netzwerk-Isolation Prüfung)
- Host Functions mit Memory-Serialization
- **Status:** ✅ Feature-Complete

---

## 3. Offener Backlog

### 3.1 Aktive Items

| ID | Crate | Titel | Priorität | Status | Beschreibung |
|---|---|---|---|---|---|
| **FIND-STO-001** | `store` | WAL CRC-Validierung | TIER 2 | 🟡 OPEN | CRC32 pro WAL-Entry fehlt. `yield_now()` in Compaction bereits implementiert. |
| **FIND-DB-002** | `db` | OTel Tracing Expansion | TIER 3 | 🟡 OPEN | `tracing::instrument` auf pub-Methoden von `MemFuse`/`Collection`. |

### 3.2 Erledigte Items (Historisch)

<details>
<summary>Alle abgeschlossenen Findings (TIER 1–3)</summary>

| ID | Crate | Titel | Abschluss |
|---|---|---|---|
| STO-UNWRAP | `store` | SSTable Zero-Panic Hardening | ✅ ParseError statt unwrap() |
| FIND-CRY-001 | `crypto` | Hardcoded HKDF Salt | ✅ Random Salt via `try_new_random_salt()` |
| FIND-CRY-002 | `crypto` | AES-GCM Nonce-Reuse | ✅ AtomicU64 + Random Prefix |
| FIND-STO-003 | `store` | Rollback-Inkonsistenz | ✅ SSTable-Rollback verifiziert |
| FIND-TXT-001 | `text` | DAG-Violation | ✅ `memfuse-store` Abhängigkeit entfernt |
| FIND-TXT-003 | `text` | BM25 Division-by-Zero | ✅ Guardrails in BM25 Scoring |
| FIND-IDX-002 | `index` | NaN/Inf Poisoning | ✅ Input-Validation |
| FIND-IDX-003 | `index` | Rebuild Threshold | ✅ Default 0.3 |
| FIND-GRA-001 | `graph` | CSR Transaction Isolation | ✅ Committed-Only Compaction |
| FIND-SBX-001 | `sandbox` | Skeleton Host Functions | ✅ Memory Serialization |
| FIND-SBX-002 | `sandbox` | Mock AirGap | ✅ Implementiert |
| FIND-PY-001 | `py` | Exception Mapping | ✅ ValueError/IOError/RuntimeError |
| FIND-COR-003 | `core` | Pure Core Violation | ✅ Audit-Clean |
| FIND-CHK-001 | `checkpoint` | Transaction Leaks | ✅ RAII Rollback |
| COL-001/002/003 | `db` | Collection CRUD | ✅ Implementiert |
| SEARCH-001 | `db` | Hybrid Search | ✅ 4-Signal Fusion |
| WP-7.1 | `text` | Markdown Chunker | ✅ In `memfuse-db` |
| WP-7.2 | `index` | HNSW Persistence | ✅ Binary Save/Load |

</details>

---

## 4. Implementierungs-Roadmap

### Phase A: Stabilisierung (Aktuell)

Verbleibende 2 Items für lückenlose Produktionsreife des Single-Node-Systems.

```
A.1  FIND-STO-001  WAL CRC           → crates/memfuse-store/src/wal.rs
A.2  FIND-DB-002   OTel Expansion     → crates/memfuse-db/src/{lib,collection}.rs
```

### Phase B: Produktions-Skalierung (Zukunft)

> Neue Features für horizontale Skalierung. **Keine Blocker** für Single-Node-Betrieb.

| ID | Feature | Neue Crate | Beschreibung |
|---|---|---|---|
| REP-001/002 | Raft Replication | `memfuse-cluster` | `openraft`-basierte Leader-Election, WAL → State Machine |
| EMB-001/002 | Auto-Embedding | `memfuse-embed` | ONNX Runtime, optionale Vektoren in Python API |
| IPC-001/002 | Zero-Copy IPC | — | FlatBuffers für `ScoredDocument`, Sandbox Memory Pointers |
| SIMD-001/002 | Quantized SIMD | — | AVX-512 `u8` Kernels, Dynamic Feature Dispatch |

---

## 5. Qualitäts-Gates

### Triple-Test-Gate

```bash
# Gate 1: Kompilierung (Lints & Warnings = Fehler)
cargo check --workspace

# Gate 2: Tests
cargo test --workspace

# Gate 3: Clippy
cargo clippy --workspace -- -D warnings

# All-in-one:
just triple-test
```

### Sovereign Core Doctrine Audit

```bash
# Zero-Panic Check
rg "unwrap\(\)|expect\(" --type rust crates/ --glob '!*/tests/*' --glob '!*test*' --glob '!*bench*'

# Unsafe Audit
rg "unsafe" --type rust crates/ | grep -v "forbid\|deny\|allow\|//\|#!\["

# TODO/Skeleton Check
rg "TODO|todo!|unimplemented!|FIXME" --type rust crates/
```

---

## 6. Referenzen

| Dokument | Pfad | Status |
|---|---|---|
| Architektur (Kurzreferenz) | `docs/ARCHITECTURE.md` | ✅ Aktiv |
| Constitution | `CONSTITUTION.md` | ✅ Aktiv |
| Agent-Protokoll | `AGENTS.md` | ✅ Aktiv |
| **Dieses Dokument** | `docs/SOURCE_OF_TRUTH.md` | ✅ **Living State** |
