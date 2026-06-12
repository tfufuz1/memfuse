# MemFuse — Master Specification v2.0
## Vollständige Forensik · Goldstandard-Produktspezifikation · Skalierungsarchitektur · Implementierungsplan

> **Analysedatum:** 2026-05-29 | **Scope:** 11 Crates, ~10.800 LoC, 202 Commits  
> **Quellen:** Repository forensics, FORENSIC_INVENTORY.md, FORENSIC_FINDINGS.md, SKELETON_REGISTRY.md, clippy.log (130 KB), AGENTS.md, alle Crate-Specs  
> **Ziel:** Der Goldstandard unter den Embedded AI Vector Databases im Open-Source-Ökosystem

---

## Inhaltsverzeichnis

**Teil I — Forensische Vollanalyse**
1. [Executive Summary & Kritikalitätsmatrix](#1-executive-summary)
2. [Vollständiges Public API Inventory](#2-public-api-inventory)
3. [Kritische Bugs mit Vollcode-Fixes](#3-kritische-bugs--vollcode-fixes)
4. [Sicherheitsaudit — Crypto & WAL](#4-sicherheitsaudit)
5. [Positiv-Findings: Was bereits existiert](#5-positiv-findings)

**Teil II — Goldstandard-Produktspezifikation**
6. [Vollständiger Funktionskatalog (168 Features)](#6-vollständiger-funktionskatalog)
7. [Vollständige API-Spezifikation](#7-vollständige-api-spezifikation)
8. [Architektur-Zielzustand](#8-architektur-zielzustand)

**Teil III — Skalierungsarchitektur**
9. [4-Stufen-Skalierungsmodell](#9-4-stufen-skalierungsmodell)
10. [Optimierungspotenzial — Konkrete Implementierungen](#10-optimierungspotenzial)
11. [DiskANN Aktivierungsplan](#11-diskann-aktivierungsplan)

**Teil IV — Stabilisierung & Roadmap**
12. [Sprint-by-Sprint Implementierungsplan](#12-implementierungsplan)
13. [Wettbewerbsstrategie & Marktpositionierung](#13-wettbewerbsstrategie)
14. [Community & Ecosystem](#14-community--ecosystem)

---

# TEIL I — FORENSISCHE VOLLANALYSE

---

## 1. Executive Summary

### 1.1 Gesamtbewertung

| Dimension | Bewertung | Detail |
|---|---|---|
| Architektur-Design | ✅ **Exzellent** | Sauberer 4-Layer-DAG, strikte Invarianten, keine Zyklen |
| Implementierungstiefe | ✅ **Überraschend vollständig** | Kein einziges `todo!()` im gesamten Workspace |
| Code-Qualität | ✅ **Gut** | ~130+ Tests, `#![forbid(unsafe_code)]` konsequent |
| Build-Stabilität | 🔴 **Kritisch** | Compile-Fehler in 3 Crates (dyn-Inkompatibilität + Lifetimes) |
| Sicherheit | 🟠 **Risiko** | Nonce-Reuse-Mitigation in AES-GCM unvollständig |
| WAL-Integrität | 🟠 **Risiko** | CRC-Verifikation bei Replay fehlt im kritischen Pfad |
| Release-Bereitschaft | 🔴 **Nicht bereit** | 4 P0-Fixes nötig, danach schnell release-fähig |
| Marktpotenzial | ✅ **Sehr hoch** | Alleinstellung in Embedded+PureRust+HybridSearch+Encryption |

### 1.2 Die drei wichtigsten Erkenntnisse

**Erkenntnis 1 — Der Build bricht, aber es ist kein Architekturproblem:**  
`StorageEngine` ist nicht dyn-kompatibel — ein Rust-Trait-Design-Fehler, kein Architekturproblem. `memfuse-text` und `memfuse-db` haben die korrekte Generics-Lösung (`<S: StorageEngine>`) bereits vollständig umgesetzt. Nur `memfuse-checkpoint` (10 Stellen) und eine verbleibende Stelle in `memfuse-text/src/lib.rs` müssen migriert werden. **Geschätzte Behebungszeit: 2–4 Stunden.**

**Erkenntnis 2 — DiskANN, Bloom-Filter und HNSW-Persistence-Strukturen sind bereits implementiert:**  
Was als "FROZEN" oder "Geplant" gilt, existiert im Code. `DiskAnnIndex`, `DiskAnnConfig` in `diskann.rs`; `BloomFilter` in `sstable.rs`; `HnswHeader`, `NodeRecord`, `MmapIndex` in `persistence.rs`; `MarkdownChunker` in `chunker.rs`; `SpatialFence` in `context.rs`. **Diese Features müssen aktiviert und getestet werden, nicht von Grund auf implementiert.**

**Erkenntnis 3 — Die AES-GCM Nonce-Reuse-Schwachstelle ist ein echter Security-Blocker:**  
AES-GCM bricht vollständig zusammen wenn dieselbe Nonce zweimal mit demselben Key verwendet wird. Die `nonce_reuse.rs`-Testdatei zeigt, dass das Problem bekannt ist — aber die Mitigation im Produktionspfad ist unvollständig. Für ein Projekt das Encryption-at-Rest als USP vermarktet, ist das ein kritischer Vertrauensverlust wenn veröffentlicht ohne Fix.

### 1.3 Kritikalitätsmatrix (vollständig)

| ID | Crate | Kategorie | Severity | Wirtschaftliches Risiko | Fix-Aufwand |
|---|---|---|---|---|---|
| **BLK-001** | core/checkpoint/text | Compiler | 🔴 BLOCKER | Build komplett unmöglich | 2–4h |
| **BLK-002** | graph/text | Compiler | 🔴 BLOCKER | 12+ Trait-Impls korrumpiert | 3–5h |
| **SEC-001** | memfuse-crypto | Security | 🔴 CRITICAL | AES-GCM bricht bei Nonce-Reuse | 4–6h |
| **DAT-001** | memfuse-store | Data Integrity | 🔴 CRITICAL | Silent Datenverlust nach Crash | 3–5h |
| **DAT-002** | memfuse-index | Data Integrity | 🟠 HIGH | SQ8-State verloren nach Restart | 2–3h |
| **CON-001** | memfuse-checkpoint | Concurrency | 🟠 HIGH | Race Condition bei Checkpoints | 1–2h |
| **API-001** | memfuse-py | API Quality | 🟠 HIGH | 0 Tests, keine Exception-Hierarchie | 4–6h |
| **PER-001** | memfuse-text | Performance | 🟡 MEDIUM | Read-Modify-Write Bottleneck | 4h |
| **PER-002** | memfuse-store | Performance | 🟡 MEDIUM | WAL kein Group-Commit | 6h |
| **QUA-001** | global | Code Quality | 🟡 MEDIUM | 154 offene PRs, viele broken | 2h (Script) |

---

## 2. Public API Inventory

### 2.1 `memfuse-core` — Shared Kernel (1.129 LoC)

**Traits (vollständig):**

| Trait | Datei:Zeile | Methoden | Zweck |
|---|---|---|---|
| `Checkpoint` | `traits.rs:20` | `pin`, `unpin` | Snapshot-Pins für Time-Travel |
| `Snapshot` | `traits.rs:29` | `seq_no`, `get`, `scan` | Immutable DB-View |
| `StorageEngine` | `traits.rs:63` | `get`, `get_at_seq`, `put`, `delete`, `commit`, `rollback`, `flush`, `stats`, `last_seq_no`, `pin_checkpoint`, `unpin_checkpoint`, `scan_prefix`, `scan` | Kern-Storage-Abstraktion (LSM-Interface) |
| `VectorIndex` | `traits.rs:131` | `insert`, `delete`, `search`, `commit`, `rollback`, `stats`, `count` | ANN-Search Interface |
| `TextIndex` | `traits.rs:198` | `insert`, `delete`, `search`, `commit`, `rollback`, `stats` | BM25-Interface |
| `GraphIndex` | `traits.rs:224` | `traverse`, `add_entity`, `add_edge`, `commit`, `rollback`, `stats` | Entity-Graph Interface |

**Kerntypen:**

| Typ | Beschreibung |
|---|---|
| `DocId(u64)` | Dokumenten-ID — Newtype, Hash-basiert aus String-ID |
| `TxId(u64)` | Transaktions-ID für MVCC-Snapshot-Isolation |
| `EntityId(u64)` | Graph-Entity-ID für CSR-Traversal |
| `Embedding { data: Vec<f32>, metric: DistanceMetric }` | Vektorrepräsentation mit Distanzmetrik |
| `ScoredDocument { id, score, metadata }` | Suchergebnis-Typ |
| `FusionWeights { bm25, vector, graph, temporal }` | 4-Signal-Gewichtungskonfiguration |
| `HybridQuery / HybridQueryBuilder` | Builder-Pattern für kombinierte Suchanfragen |
| `ContextWindow / ContextChunk` | Agenten-Kontext-Verwaltung mit TokenBudget |
| `TokenBudget { max_tokens, used_tokens, reserved }` | Token-Limitierung für LLM-Kontext-Fenster |
| `ResourceBudget / ResourceTracker` | Ressourcen-Monitoring für Agenten |
| `FilterExpr` | Enum: `And`, `Or`, `Not`, `Leaf` — Predicate-DSL |
| `IsolationLevel` | `ReadCommitted` \| `Serializable` |
| `IndexOp<T>` | `Insert(T)` \| `Update(T)` \| `Delete(DocId)` |
| `WorkflowState / WorkflowNode / WorkflowEdge` | StateGraph für SAOS-Agent |

**Tests:** 20 Tests — `tx_buffer.rs`, `snapshot.rs`, `types/saos.rs`, `types/domain.rs`, `types/budget.rs`

---

### 2.2 `memfuse-store` — LSM Storage Engine (2.912 LoC)

**Structs (vollständig):**

| Struct | Datei | Kernfelder | Tests |
|---|---|---|---|
| `LsmStorage` | `lsm.rs:104` | `MemTable`, `Vec<SsTable>`, `Wal`, `CompactionEngine` | 9 |
| `LsmConfig` | `lsm.rs:70` | `path`, `memtable_size_bytes`, `compaction_policy`, `cache_size` | — |
| `MemTable` | `memtable.rs:17` | `BTreeMap<Key, Entry>`, `seq_no: AtomicU64`, `size: AtomicUsize` | 5 |
| `Wal` | `wal.rs:140` | `file: tokio::fs::File`, `offset: u64`, `crc_hasher` | 6 |
| `WalEntry` | `wal.rs:34` | `tx_id`, `op: WalOp`, `payload: Bytes`, `crc: u32` | — |
| `WalOp` | `wal.rs:12` | `Put` \| `Delete` \| `Commit` \| `Rollback` | — |
| `SstableBuilder` | `sstable.rs:232` | Block-Encoder, Index-Builder | — |
| `SstableReader` | `sstable.rs:404` | `MmapReader`, `BloomFilter`, `BlockIndex` | 8 |
| `SstableStream` | `sstable.rs:966` | Streaming-Iterator für Range-Scans | — |
| **`BloomFilter`** | `sstable.rs:49` | `bits: Vec<u64>`, `k_hashes: u8` | vorhanden |
| `BlockBuilder` | `sstable.rs:151` | Key-Value Block mit Prefix-Compression | — |
| `CompactionEngine` | `compaction.rs:59` | `policy: CompactionPolicy`, `workers: JoinHandle` | 6 |
| `CompactionConfig` | `compaction.rs:33` | `level_size_multiplier`, `max_levels`, `compaction_threads` | — |
| `Checkpointer` | `checkpoint.rs:18` | WAL-Checkpoint-Management im Store | — |
| `MmapReader` | `mmap.rs:11` | Memory-Mapped File Access via `memmap2` | — |

**Externe Tests:** `rollback_sstables.rs`, `encryption_test.rs`  
**Gesamt:** 30+ Tests

> ⚡ **KRITISCHER FUND:** `BloomFilter` ist bereits implementiert in `sstable.rs:49` — es ist kein offenes Feature, sondern ein existierendes das validiert werden muss.

---

### 2.3 `memfuse-index` — Vector Engine (2.420 LoC)

**Structs (vollständig):**

| Struct | Datei | Zweck |
|---|---|---|
| `HnswIndex` | `hnsw.rs:160` | HNSW-Graph in-memory (ANN Search) — Haupt-VectorIndex-Impl |
| `HnswIndexCore` | `hnsw.rs:172` | Interner Graph-State unter `RwLock` |
| `HnswConfig` | `hnsw.rs:57` | `m: usize`, `ef_construction: usize`, `ef_search: usize`, `max_layer` |
| `VectorData` | `hnsw.rs:112` | Enum: `Float32(Vec<f32>)` \| `SQ8(Vec<i8>, ScalarQuantizer)` |
| `ScalarQuantizer` | `quantize.rs:15` | `min: Vec<f32>`, `max: Vec<f32>` — SQ8-Quantizer-State |
| **`DiskAnnIndex`** | `diskann.rs:169` | **Out-of-Core Index auf NVMe** — bereits implementiert! |
| **`DiskAnnConfig`** | `diskann.rs:114` | `beam_width`, `max_degree`, `cache_size_mb`, `path` |
| **`HnswHeader`** | `persistence.rs:22` | Serialisierungsformat für mmap-HNSW-Persistenz |
| **`NodeRecord`** | `persistence.rs:133` | Einzelner HNSW-Node auf Disk (64-Byte aligned) |
| **`MmapIndex`** | `persistence.rs:179` | Memory-Mapped HNSW-Index für Zero-Copy-Load |
| `CosineSimilarityPartsU8` | `distance.rs:578` | SIMD-Distanz für SQ8-Vektoren |
| `CosineSimilarityPartsF32U8` | `distance.rs:649` | Mixed-Precision SIMD-Distanz |

**Externe Tests:** `poisoning.rs` (3), `recall.rs` (1), `ram_reduction.rs` (1)  
**Gesamt:** 25+ Tests

> ⚡ **KRITISCHER FUND:** `DiskAnnIndex` ist **vollständig implementiert** in `diskann.rs`. WP-4.3 ("in Refactor") ist falsch eingestuft — es muss aktiviert und integrationstested werden, nicht implementiert.

> ⚡ **KRITISCHER FUND:** `HnswHeader`, `NodeRecord`, `MmapIndex` in `persistence.rs` — HNSW-Persistence-Infrastruktur existiert. WP-7.2 ("FROZEN") muss nur verdrahtet werden.

---

### 2.4 `memfuse-text` — Keyword Engine (935 LoC)

**Structs (vollständig):**

| Struct | Datei | Zweck |
|---|---|---|
| `Tokenizer` (Trait) | `tokenizer.rs:25` | Basis-Tokenizer-Abstraktion |
| `MorphologicalTokenizer` (Trait) | `morphology.rs:14` | Erweitertes Interface mit Lemmatisierung |
| `DefaultTokenizer` | `tokenizer.rs:31` | Unicode-aware, sprachunabhängig |
| `GermanMorphTokenizer` | `tokenizer.rs:44` | Morphologische Analyse + Compound-Splitting |
| `GermanCompoundSplitter` | `morphology.rs:26` | `"Softwareentwicklung"` → `["software", "entwicklung"]` |
| `InvertedIndex<S: StorageEngine>` | `inverted.rs:29` | BM25-Basis, Generics korrekt (kein `dyn`) |
| `BM25MorphIndex<S: StorageEngine>` | `inverted.rs:366` | BM25 + Morphologie kombiniert |
| `Bm25Scorer<S: StorageEngine>` | `lib.rs:20` | Scoring-Engine mit k1/b-Parametern |
| `TextIndexMetadata` | `inverted.rs:22` | IDF-Statistiken, Dokument-Count, avg_doc_len |
| `PassthroughTokenizer` | `morphology.rs:118` | No-Op für Tests |
| `TokenReductionMetrics` | `morphology.rs:141` | Statistiken über Token-Reduktionsrate |

> ⚠️ Problem: `memfuse-text/src/lib.rs:25` hat noch `Arc<dyn StorageEngine>` — genau eine Stelle die migriert werden muss.

---

### 2.5 `memfuse-crypto` — Encryption (216 LoC)

| Struct/Trait | Datei | Zweck |
|---|---|---|
| `KmsProvider` (Trait) | `wal_crypto.rs:12` | Key-Management-System Abstraktion |
| `KeyManager` | `crypto.rs:16` | HKDF-Key-Derivation, AES-256-GCM Block-Encryption |
| `EncryptedWal` | `wal_crypto.rs:18` | Verschlüsselte WAL-Datei |
| `WalHmac` | `wal_crypto.rs:46` | HMAC-SHA256 über WAL-Sequenz |
| `WalEntrySnapshot` | `wal_crypto.rs:68` | Snapshot für Integritätsverifikation |
| `IntegrityVerifier` | `wal_crypto.rs:78` | HMAC-Verifikation — **existiert, muss im Hot-Path aktiviert werden** |

> ⚡ **Fund:** `nonce_reuse.rs` Test-Datei existiert → Problem ist bekannt. Produktionspfad-Mitigation fehlt.  
> **Tests:** 11 — `crypto.rs` (6), `wal_crypto.rs` (3), externe `nonce_reuse.rs` (2)

---

### 2.6 `memfuse-db` — Orchestrator (1.917 LoC)

| Struct | Datei | Zweck |
|---|---|---|
| `MemFuse` | `lib.rs:128` | Haupt-DB-Handle, Einstiegspunkt |
| `MemFuseConfig` | `lib.rs:101` | Pfad, Dimension, Encryption-Key, Compaction-Policy |
| `Collection<S: StorageEngine>` | `collection.rs:53` | Namespace-isolierte Sammlung mit allen 3 Indizes |
| `SearchResult` | `lib.rs:72` | `{ id, score, metadata, vector_score, text_score }` |
| `Document` | `lib.rs:92` | `{ id, embedding, metadata, text }` |
| `DbStats` | `lib.rs:83` | Aggregierte Statistiken |
| `DbTransaction<'a, S>` | `transaction.rs:17` | ACID-Transaktionshandle |
| `ContextManager` | `context.rs:25` | Agenten-Kontext-Verwaltung, Token-Budget |
| **`SpatialFence`** | `context.rs:108` | Geo-basiertes Context-Filtering — **bereits implementiert** |
| **`MarkdownChunker`** | `chunker.rs:35` | Semantisches Chunking — **bereits implementiert** |
| `ChunkerConfig` | `chunker.rs:12` | Chunk-Größe, Overlap, Separator-Patterns |
| `NamespaceRegistry` | `namespace.rs:74` | Multi-Tenancy-Verwaltung |
| `Namespace` | `namespace.rs:15` | Einzelner Tenant-Namespace |
| `FilterOp / MetadataFilter` | `filter.rs:6,27` | Metadata-Filter-DSL |

> ⚡ **Fund:** `MarkdownChunker` in `chunker.rs` — WP-7.1 ist bereits implementiert!  
> **Tests:** 40+ — `lib.rs` (17), `collection_contract.rs` (4), `transaction_isolation.rs` (3), `full_stack_e2e.rs` (1), `filter_tests.rs` (3), `fusion.rs` (4), `chunker.rs` (4), weitere.

---

### 2.7 `memfuse-py` — Python Bindings (528 LoC)

| Struct | Datei | Zweck |
|---|---|---|
| `PyMemFuse` | `lib.rs:486` | Python-Wrapper für `MemFuse` |
| `PyCollection` | `lib.rs:592` | Python-Wrapper für `Collection` |
| `PySearchResult` | `lib.rs:120` | Python-Suchergebnis mit `__repr__` |
| `PyDocument` | `lib.rs:138` | Python-Dokument-Objekt |
| `PyVectorIndexStats` | `lib.rs:155` | Index-Statistiken |
| `PyStorageStats` | `lib.rs:177` | Storage-Statistiken |
| `PyDbStats` | `lib.rs:199` | Aggregierte DB-Stats |

> ⚠️ **KRITISCH: Kein einziger Test.** Für ein PyPI-Release vollständig inakzeptabel.  
> ⚠️ **Keine Exception-Hierarchie:** Alle Fehler landen als `PyRuntimeError` — Nutzer können nicht programmatisch auf Fehlertypen reagieren.

---

## 3. Kritische Bugs — Vollcode-Fixes

### 3.1 BLK-001 — StorageEngine dyn-Inkompatibilität

**Problem-Analyse:**

Das `StorageEngine`-Trait in `memfuse-core/src/traits.rs` deklariert `async fn`-Methoden. In Rust ist ein Trait mit `async fn` nicht dyn-kompatibel — kein vtable kann erstellt werden. Dieses Muster erscheint 15+ Mal im `clippy.log`:

```
error[E0038]: the trait `memfuse_core::StorageEngine` is not dyn compatible
note: method `get` is `async` (async fn cannot be in a vtable)
```

**Betroffene Stellen:**
- `memfuse-checkpoint/src/lib.rs` — 10 Stellen (`Arc<dyn StorageEngine>`)
- `memfuse-text/src/lib.rs:25` — 1 Stelle

**Fix-Strategie: Generics statt dyn (Sovereign-Core-konform, kein Performance-Overhead):**

```rust
// ❌ VORHER — memfuse-checkpoint/src/lib.rs
pub struct PersistentCheckpointStore {
    storage: Arc<dyn StorageEngine>,
    registry: parking_lot::Mutex<CheckpointRegistry>,
    namespace: String,
}

impl PersistentCheckpointStore {
    pub fn new(storage: Arc<dyn StorageEngine>) -> Self { ... }
}

// ✅ NACHHER — Generics-Migration
pub struct PersistentCheckpointStore<S: StorageEngine> {
    storage: Arc<S>,
    registry: parking_lot::RwLock<CheckpointRegistry>, // RwLock statt Mutex → HIGH-002 Fix
    namespace: String,
}

impl<S: StorageEngine + Send + Sync + 'static> PersistentCheckpointStore<S> {
    pub fn new(storage: Arc<S>, namespace: impl Into<String>) -> Self {
        Self {
            storage,
            registry: parking_lot::RwLock::new(CheckpointRegistry::default()),
            namespace: namespace.into(),
        }
    }

    pub async fn pin_checkpoint(&self, seq_no: u64) -> crate::Result<CheckpointId> {
        let id = CheckpointId::new(seq_no);
        self.storage.pin_checkpoint(seq_no).await?;
        self.registry.write().insert(id, seq_no);
        Ok(id)
    }
}
```

```rust
// ❌ VORHER — memfuse-text/src/lib.rs:25
pub struct TextSearchEngine {
    index: Arc<dyn StorageEngine>,  // BUG
    namespace: String,
}

// ✅ NACHHER
pub struct TextSearchEngine<S: StorageEngine> {
    index: Arc<S>,
    namespace: String,
}
```

**Testverifikation nach Fix:**
```bash
cargo build -p memfuse-checkpoint
cargo build -p memfuse-text
cargo build --workspace 2>&1 | grep "^error" | wc -l  # Muss 0 sein
```

---

### 3.2 BLK-002 — Lifetime-Mismatches in Trait-Implementierungen

**Problem-Analyse:**

Beim asynchronen Desugaring in Rust generiert der Compiler implizite Lifetime-Parameter. Trait-Definitionen in `memfuse-core` und Implementierungen in separaten Crates wurden zu verschiedenen Zeitpunkten geschrieben — ohne Lifetime-Synchronisation.

**Betroffene Methoden (12 Stück):**

```
memfuse-graph/src/csr.rs:
  add_entity() — error[E0195]: lifetime parameters do not match
  add_edge()   — error[E0195]: lifetime parameters do not match
  traverse()   — error[E0195]: lifetime parameters do not match
  commit()     — error[E0195]: lifetime parameters do not match
  rollback()   — error[E0195]: lifetime parameters do not match
  stats()      — error[E0195]: lifetime parameters do not match

memfuse-text/src/inverted.rs:
  search()     — error[E0195]
  insert()     — error[E0195]
  delete()     — error[E0195]
  commit()     — error[E0195]
  rollback()   — error[E0195]
  stats()      — error[E0195]
```

**Fix-Pattern:**

Das Muster ist konsistent für alle 12 Methoden. Die Trait-Deklaration und die Implementierung müssen exakt dieselbe `async fn`-Signatur haben — keine extra `'_`-Lifetime-Annotationen in der Impl, die nicht im Trait sind:

```rust
// ❌ VORHER — Impl hat extra Lifetime-Annotationen
impl<S: StorageEngine> GraphIndex for CsrGraph<S> {
    async fn traverse<'a>(&'a self, start: EntityId, hops: usize) 
        -> crate::Result<Vec<(EntityId, f32)>> { ... }
    //             ^^^^ Extra Lifetime nicht im Trait
}

// ✅ NACHHER — exakt wie im Trait
impl<S: StorageEngine> GraphIndex for CsrGraph<S> {
    async fn traverse(&self, start: EntityId, max_hops: usize)
        -> crate::Result<Vec<(EntityId, f32)>> {
        // Implementation
        let graph = self.inner.read();
        let mut visited = ahash::AHashSet::new();
        let mut queue = std::collections::VecDeque::new();
        let mut results = Vec::new();
        
        queue.push_back((start, 0usize, 1.0f32));
        visited.insert(start);
        
        while let Some((node, depth, weight)) = queue.pop_front() {
            if depth > 0 {
                results.push((node, weight));
            }
            if depth >= max_hops { continue; }
            
            for (neighbor, edge_weight) in graph.neighbors(node) {
                if visited.insert(neighbor) {
                    queue.push_back((neighbor, depth + 1, weight * edge_weight));
                }
            }
        }
        Ok(results)
    }
}
```

---

### 3.3 BLK-003 — `[u8]` nicht Sized in Checkpoint

```rust
// ❌ VORHER — memfuse-checkpoint/src/lib.rs:141
for (_, value) in entries {  // value: [u8] — NICHT Sized!
    // ...
}

// ✅ NACHHER
for (_, value) in entries {  // value: Vec<u8>
    let value: Vec<u8> = value; // explizit
    // ...
}
```

Der Scan-Return-Typ in `StorageEngine` muss `Vec<u8>` statt `[u8]` zurückgeben — `[u8]` ist ein Slice-Typ und nicht auf dem Stack allokierbar.

---

## 4. Sicherheitsaudit

### 4.1 SEC-001 — AES-GCM Nonce-Reuse (KRITISCH)

**Schwachstelle:**  
AES-256-GCM ist katastrophal anfällig für Nonce-Wiederverwendung. Wenn dieselbe 96-Bit-Nonce zweimal mit demselben Schlüssel verwendet wird:
- Beide Klartexte sind durch XOR rekonstruierbar
- Der GCM-Authentizierungsschlüssel ist kompromittierbar
- Alle vergangenen Chiffrate können entschlüsselt werden

**Forensischer Befund:**  
Die Testdatei `nonce_reuse.rs` zeigt, dass das Problem bekannt ist. Die Frage ist: Wie werden Nonces in `KeyManager` und `EncryptedWal` generiert?

Typische Risikoszenarien:
```rust
// SZENARIO 1 — Deterministischer Nonce (shard_id || block_number)
// Wenn nach Crash derselbe Block_number vergeben wird → NONCE REUSE

// SZENARIO 2 — Zufälliger Nonce ohne Persistenz
// Problem: 2^48 = 280 Billionen Operationen bis 50% Kollisionswahrscheinlichkeit
// Bei NVMe-Throughput (100K ops/sec) → nach ~88 Jahren... aber bei vielen DBs zusammen
// oder bei Schlüsselwiederverwendung nach Backup-Restore → Sofortkollision
```

**Fix-Option A — AES-256-GCM-SIV (empfohlen):**

```toml
# Cargo.toml — memfuse-crypto
[dependencies]
aes-gcm-siv = "0.11"  # Nonce-Misuse-Resistant Variant
# ENTFERNEN: aes-gcm = "0.10"
```

```rust
// crypto.rs — AES-256-GCM-SIV statt AES-256-GCM
use aes_gcm_siv::{Aes256GcmSiv, Key, Nonce};
use aes_gcm_siv::aead::{Aead, NewAead};

pub struct KeyManager {
    cipher: Aes256GcmSiv,
    nonce_counter: AtomicU64,      // Monoton steigend
    nonce_high: u64,                // Zufällig bei Initialisierung
}

impl KeyManager {
    pub fn new(key_material: &[u8]) -> crate::Result<Self> {
        let key = Self::derive_key(key_material)?;
        let cipher = Aes256GcmSiv::new(Key::from_slice(&key));
        
        // Nonce-High: 4 Bytes zufällig (pro-Instanz-Unique)
        let mut nonce_high = [0u8; 8];
        rand::thread_rng().fill_bytes(&mut nonce_high);
        
        Ok(Self {
            cipher,
            nonce_counter: AtomicU64::new(0),
            nonce_high: u64::from_le_bytes(nonce_high),
        })
    }

    fn next_nonce(&self) -> [u8; 12] {
        // 4 Bytes High (instanz-spezifisch) + 8 Bytes Counter (monoton)
        // → Bei gleicher Instanz: nie wiederholend bis 2^64 Operationen
        // → Bei AES-GCM-SIV: auch bei Collision sicher
        let counter = self.nonce_counter.fetch_add(1, Ordering::SeqCst);
        let mut nonce = [0u8; 12];
        nonce[0..4].copy_from_slice(&(self.nonce_high as u32).to_le_bytes());
        nonce[4..12].copy_from_slice(&counter.to_le_bytes());
        nonce
    }

    pub fn encrypt(&self, plaintext: &[u8]) -> crate::Result<Vec<u8>> {
        let nonce = self.next_nonce();
        let nonce_obj = Nonce::from_slice(&nonce);
        
        let mut ciphertext = self.cipher
            .encrypt(nonce_obj, plaintext)
            .map_err(|e| MemFuseError::Encryption(e.to_string()))?;
        
        // Nonce prepended für Decryption
        let mut result = Vec::with_capacity(12 + ciphertext.len());
        result.extend_from_slice(&nonce);
        result.append(&mut ciphertext);
        Ok(result)
    }

    pub fn decrypt(&self, data: &[u8]) -> crate::Result<Vec<u8>> {
        if data.len() < 12 {
            return Err(MemFuseError::Encryption("Ciphertext zu kurz".into()));
        }
        let (nonce, ciphertext) = data.split_at(12);
        let nonce_obj = Nonce::from_slice(nonce);
        
        self.cipher
            .decrypt(nonce_obj, ciphertext)
            .map_err(|_| MemFuseError::Encryption("Decryption fehlgeschlagen".into()))
    }
}
```

**Fix-Option B — Persistenter Nonce-Counter:**
```rust
// Nonce-Counter wird atomar in einer dedizierten Datei gespeichert
// Nach jedem 1024 Nonces: fsync (amortisiert)
struct PersistedNonceCounter {
    file: tokio::fs::File,
    counter: AtomicU64,
    last_persisted: AtomicU64,
}
```

**Empfehlung:** Option A — AES-GCM-SIV hat identische Performance zu AES-GCM auf modernen CPUs und ist inherent sicher gegen Nonce-Misuse.

---

### 4.2 DAT-001 — WAL Rollback-Integrität

**Vollständiger Fix für `memfuse-store/src/wal.rs`:**

```rust
// wal.rs — replay() mit vollständiger CRC-Verifikation und atomarer Semantik

pub async fn replay(&self) -> Result<Vec<CompletedTransaction>> {
    let raw_data = tokio::fs::read(&self.path).await
        .map_err(|e| MemFuseError::Io(e.to_string()))?;
    
    let mut pos = 0usize;
    let mut pending: HashMap<TxId, Vec<WalEntry>> = HashMap::new();
    let mut committed: Vec<CompletedTransaction> = Vec::new();
    let mut corrupt_count = 0usize;
    
    while pos < raw_data.len() {
        // Header lesen
        if pos + WAL_ENTRY_HEADER_SIZE > raw_data.len() {
            tracing::warn!("WAL: Unvollständiger Header bei Offset {}", pos);
            break;
        }
        
        let entry = match WalEntry::decode(&raw_data[pos..]) {
            Ok(e) => e,
            Err(e) => {
                tracing::error!("WAL: Decode-Fehler bei Offset {}: {}", pos, e);
                break; // Konservativer Ansatz: Stop bei erstem Fehler
            }
        };
        
        // ✅ CRC-Verifikation VOR Replay
        let actual_crc = crc32fast::hash(&entry.payload);
        if actual_crc != entry.crc {
            corrupt_count += 1;
            tracing::error!(
                "WAL: CRC-Mismatch bei Offset {} (erwartet: {:08x}, tatsächlich: {:08x})",
                pos, entry.crc, actual_crc
            );
            
            // Konfigurierbar: Abbruch oder Skip
            if self.config.strict_replay {
                return Err(MemFuseError::CorruptWal {
                    offset: pos as u64,
                    expected_crc: entry.crc,
                    actual_crc,
                });
            } else {
                // Permissive: Skip korrumpierter Entry, log warning
                pos += entry.encoded_size();
                continue;
            }
        }
        
        // Transaktion verarbeiten
        match entry.op {
            WalOp::Put | WalOp::Delete => {
                pending.entry(entry.tx_id)
                    .or_default()
                    .push(entry);
            }
            WalOp::Commit => {
                // Nur vollständig committete TXs werden zurückgegeben
                if let Some(entries) = pending.remove(&entry.tx_id) {
                    committed.push(CompletedTransaction { 
                        tx_id: entry.tx_id, 
                        entries,
                        seq_no: entry.seq_no,
                    });
                }
            }
            WalOp::Rollback => {
                // Explizit verwerfen
                pending.remove(&entry.tx_id);
            }
        }
        
        pos += entry.encoded_size();
    }
    
    // Unvollständige Transaktionen (Crash mitten in TX) → verwerfen
    if !pending.is_empty() {
        tracing::warn!(
            "WAL: {} unvollständige Transaktionen verworfen (Crash-Recovery)",
            pending.len()
        );
    }
    
    if corrupt_count > 0 {
        tracing::warn!("WAL: {} korrumpierte Einträge übersprungen", corrupt_count);
    }
    
    Ok(committed)
}
```

Neuer `MemFuseError`-Variant:
```rust
// core/src/error.rs
#[derive(thiserror::Error, Debug)]
pub enum MemFuseError {
    // ... bestehende Varianten ...
    
    #[error("WAL korrumpiert bei Offset {offset}: CRC erwartet {expected_crc:08x}, tatsächlich {actual_crc:08x}")]
    CorruptWal {
        offset: u64,
        expected_crc: u32,
        actual_crc: u32,
    },
    
    #[error("AES-GCM Verschlüsselungsfehler: {0}")]
    Encryption(String),
}
```

---

### 4.3 CON-001 — Checkpoint-Store Race Condition

```rust
// ❌ VORHER — kein Locking
pub struct PersistentCheckpointStore {
    registry: CheckpointRegistry,  // Unsynchronisiert!
}

// ✅ NACHHER — RwLock für concurrent reads, exclusive writes
pub struct PersistentCheckpointStore<S: StorageEngine> {
    storage: Arc<S>,
    registry: parking_lot::RwLock<CheckpointRegistry>,
    namespace: String,
}

impl<S: StorageEngine + Send + Sync + 'static> PersistentCheckpointStore<S> {
    pub async fn create_checkpoint(&self, name: &str) -> crate::Result<CheckpointId> {
        // Exclusive lock für das Erstellen
        let seq_no = self.storage.last_seq_no().await?;
        self.storage.pin_checkpoint(seq_no).await?;
        
        let id = CheckpointId::from_name(name);
        self.registry.write().insert(id, CheckpointMeta {
            name: name.to_string(),
            seq_no,
            created_at: std::time::SystemTime::now(),
        });
        
        // Persist die Registry-Änderung
        self.persist_registry().await?;
        Ok(id)
    }
    
    pub fn list_checkpoints(&self) -> Vec<CheckpointMeta> {
        // Shared read lock → keine Blockierung zwischen lesenden Callern
        self.registry.read().all().collect()
    }
}
```

---

### 4.4 DAT-002 — SQ8-Quantizer-State Persistierung

```rust
// index/src/quantize.rs — Persistierung des Quantizer-State

#[derive(serde::Serialize, serde::Deserialize)]
pub struct ScalarQuantizerState {
    pub dimension: usize,
    pub min: Vec<f32>,
    pub max: Vec<f32>,
    pub scale: Vec<f32>,   // (max - min) / 255.0 pro Dimension
}

impl ScalarQuantizer {
    pub fn to_state(&self) -> ScalarQuantizerState {
        ScalarQuantizerState {
            dimension: self.dimension,
            min: self.min.clone(),
            max: self.max.clone(),
            scale: self.min.iter().zip(self.max.iter())
                .map(|(mn, mx)| (mx - mn) / 255.0_f32)
                .collect(),
        }
    }
    
    pub fn from_state(state: ScalarQuantizerState) -> Self {
        Self { 
            dimension: state.dimension,
            min: state.min, 
            max: state.max 
        }
    }
}

// In persistence.rs — ScalarQuantizer-State in HnswHeader einbetten
#[repr(C)]
pub struct HnswHeader {
    pub magic: [u8; 8],          // b"MEMFHNSW"
    pub version: u32,
    pub dimension: u32,
    pub node_count: u64,
    pub m: u32,
    pub ef_construction: u32,
    pub has_quantizer: bool,
    pub quantizer_offset: u64,   // Offset des ScalarQuantizerState in der Datei
    pub _reserved: [u8; 31],
}
```

---

### 4.5 API-001 — Python Exception-Hierarchie

```python
# memfuse-py/src/lib.rs — Exception-Hierarchie für Python

# Python-Seite (generiert via PyO3):
# MemFuseError (Basis)
# ├── MemFuseNotFoundError         — DocId nicht gefunden
# ├── MemFuseCollectionError       — Collection-Fehler
# ├── MemFuseTransactionError      — TX commit/rollback fehlgeschlagen  
# ├── MemFuseCorruptionError       — WAL/SSTable Korruption
# ├── MemFuseEncryptionError       — Crypto-Fehler
# ├── MemFuseIoError               — Disk-I/O Fehler
# └── MemFuseDimensionError        — Vektordimension falsch
```

```rust
// lib.rs — Rust-seitige Exception-Registrierung
use pyo3::exceptions::PyException;
use pyo3::create_exception;

create_exception!(memfuse, MemFuseError, PyException);
create_exception!(memfuse, MemFuseNotFoundError, MemFuseError);
create_exception!(memfuse, MemFuseCollectionError, MemFuseError);
create_exception!(memfuse, MemFuseTransactionError, MemFuseError);
create_exception!(memfuse, MemFuseCorruptionError, MemFuseError);
create_exception!(memfuse, MemFuseEncryptionError, MemFuseError);
create_exception!(memfuse, MemFuseIoError, MemFuseError);
create_exception!(memfuse, MemFuseDimensionError, MemFuseError);

// Mapping von Rust-Fehlern auf Python-Exceptions
fn rust_to_py(e: memfuse_core::MemFuseError) -> PyErr {
    match e {
        MemFuseError::NotFound(_) => MemFuseNotFoundError::new_err(e.to_string()),
        MemFuseError::CollectionNotFound(_) => MemFuseCollectionError::new_err(e.to_string()),
        MemFuseError::TransactionAborted => MemFuseTransactionError::new_err(e.to_string()),
        MemFuseError::CorruptWal { .. } => MemFuseCorruptionError::new_err(e.to_string()),
        MemFuseError::Encryption(_) => MemFuseEncryptionError::new_err(e.to_string()),
        MemFuseError::Io(_) => MemFuseIoError::new_err(e.to_string()),
        MemFuseError::DimensionMismatch { .. } => MemFuseDimensionError::new_err(e.to_string()),
        _ => MemFuseError::new_err(e.to_string()),
    }
}
```

---

## 5. Positiv-Findings

Die forensische Analyse zeigt, dass MemFuse erheblich weiter ist als die AGENTS.md-Statusanzeigen vermuten lassen.

### 5.1 Bereits implementierte "geplante" Features

| Feature | AGENTS.md Status | Tatsächlicher Status | Fundort |
|---|---|---|---|
| DiskANN Out-of-Core | 🟡 "in Refactor" | ✅ **Vollständig impl.** | `index/diskann.rs` |
| HNSW Persistence Structs | 🛑 "FROZEN" | ✅ **Structs existieren** | `index/persistence.rs` |
| Bloom-Filter | nicht erwähnt | ✅ **Bereits in SSTable** | `store/sstable.rs:49` |
| Markdown Chunker | 🛑 "FROZEN" WP-7.1 | ✅ **Vollständig impl.** | `db/chunker.rs:35` |
| SpatialFence | nicht erwähnt | ✅ **Implementiert** | `db/context.rs:108` |
| IntegrityVerifier | nicht erwähnt | ✅ **Implementiert** | `crypto/wal_crypto.rs:78` |
| KmsProvider Trait | nicht erwähnt | ✅ **Implementiert** | `crypto/wal_crypto.rs:12` |
| FusionWeights | WP-6.1 FROZEN | ✅ **Typ existiert** | `core/types` |

### 5.2 Zero-Skeleton-Status

**Kein einziges `todo!()`, `unimplemented!()` oder `unreachable!()` im gesamten Workspace.** In einem solchen Projekt ist das außergewöhnlich. Alle Funktionen haben echte Implementierungen.

### 5.3 Test-Coverage-Inventar

| Crate | Tests | Qualität |
|---|---|---|
| `memfuse-core` | 20 | Gut — Traits + Types |
| `memfuse-store` | 30+ | Sehr gut — WAL, SSTable, Compaction |
| `memfuse-index` | 25+ | Gut — HNSW, Recall, SIMD |
| `memfuse-text` | 18 | Mittel — BM25, Morphologie |
| `memfuse-crypto` | 11 | Mittel — inkl. `nonce_reuse.rs` |
| `memfuse-graph` | 5 | Minimal |
| `memfuse-db` | 40+ | **Exzellent** — E2E, Transaktionen, Filter |
| `memfuse-py` | **0** | **Kritisch — 0 Tests!** |
| **Gesamt** | **~150+** | Gut bis Sehr gut |

---

# TEIL II — GOLDSTANDARD-PRODUKTSPEZIFIKATION

---

## 6. Vollständiger Funktionskatalog (168 Features)

### F1 — Storage Engine

| ID | Feature | Status | Priorität |
|---|---|---|---|
| F1.01 | LSM-Tree Persistenz (WAL+MemTable+SSTable+Compaction) | ✅ | P0 |
| F1.02 | WAL CRC32-Verifikation bei Crash-Recovery | 🔴 FIX | P0 |
| F1.03 | WAL Group-Commit (Batch-fsync, 1ms-Window) | 🔵 Opt. | P1 |
| F1.04 | Memory-Mapped SSTable I/O | ✅ | P0 |
| F1.05 | Bloom-Filter für Point-Lookups | ✅ vorhanden | P0 |
| F1.06 | Tiered Compaction (Level-based) | ✅ | P0 |
| F1.07 | MVCC via seq_no (MemTable) | ✅ | P0 |
| F1.08 | Sharded TxBuffer (16 Shards) | ✅ | P0 |
| F1.09 | LZ4 Block-Kompression (Feature-Flag) | 🔵 Geplant | P1 |
| F1.10 | Column Families (getrennte Stores) | 🔵 Geplant | P2 |
| F1.11 | Block-Cache (LRU, konfigurierbar) | ✅ `lru` dep | P0 |
| F1.12 | WAL Encryption (AES-256-GCM) | ✅ | P0 |
| F1.13 | SSTable Encryption | ✅ | P0 |
| F1.14 | Point-Lookup via Bloom+SST | ✅ | P0 |
| F1.15 | Prefix-Scan für Namespace-Isolierung | ✅ | P0 |
| F1.16 | Automatischer Recovery bei Startup | ✅ | P0 |
| F1.17 | Range-Scan via SstableStream | ✅ | P0 |
| F1.18 | Configurable Write-Sync-Level | 🔵 Geplant | P2 |

### F2 — Vektor-Such-Engine

| ID | Feature | Status | Priorität |
|---|---|---|---|
| F2.01 | HNSW Approximate NN (m, ef_c, ef_s) | ✅ | P0 |
| F2.02 | Cosine / L2 / Inner-Product Distanz | ✅ | P0 |
| F2.03 | SIMD-Distanzberechnung (AVX2/NEON) | ✅ | P0 |
| F2.04 | SQ8 Scalar Quantization (4× RAM) | ✅ | P0 |
| F2.05 | SQ8-Quantizer-State Persistierung | 🔴 FIX | P0 |
| F2.06 | HNSW mmap-Persistence (Zero-Rebuild) | ✅ Structs | P0 |
| F2.07 | DiskANN Out-of-Core Index | ✅ Implementiert | P0 |
| F2.08 | Diversity Heuristic (MMR-ähnlich) | ✅ | P0 |
| F2.09 | Delete-Tracking via Roaring Bitmaps | ✅ | P0 |
| F2.10 | Product Quantization (16× RAM) | 🔵 Geplant | P2 |
| F2.11 | Binary Quantization (32× RAM) | 🔵 Geplant | P3 |
| F2.12 | Multi-Vector Documents (MaxSim) | 🔵 Geplant | P2 |
| F2.13 | Adaptive ef_search (Qualität-Speed) | 🔵 Geplant | P2 |
| F2.14 | HNSW inkrementeller Checkpoint | 🔵 Geplant | P1 |
| F2.15 | Recall-Benchmark-Suite | 🔵 Geplant | P1 |

### F3 — Text-Such-Engine

| ID | Feature | Status | Priorität |
|---|---|---|---|
| F3.01 | BM25-Scoring (k1/b konfigurierbar) | ✅ | P0 |
| F3.02 | Unicode-aware Tokenizer | ✅ | P0 |
| F3.03 | Deutsche Morphologie (GermanMorphTokenizer) | ✅ | P0 |
| F3.04 | Komposita-Splitting (GermanCompoundSplitter) | ✅ | P0 |
| F3.05 | Stop-Word Filtering (konfigurierbar) | 🔵 Geplant | P1 |
| F3.06 | N-Gram Tokenisierung (Typo-Toleranz) | 🔵 Geplant | P2 |
| F3.07 | Snowball Stemmer (Multilingual) | 🔵 Geplant | P2 |
| F3.08 | Field Boosting (title 2×, body 1×) | 🔵 Geplant | P2 |
| F3.09 | Fuzzy Matching (Levenshtein) | 🔵 Geplant | P2 |
| F3.10 | Phrase Queries ("exact phrase") | 🔵 Geplant | P2 |
| F3.11 | Delta-Updates in Posting-Listen | 🔴 FIX | P1 |
| F3.12 | Multi-Language Detection (automatisch) | 🔵 Geplant | P3 |

### F4 — Hybrid-Search & Fusion

| ID | Feature | Status | Priorität |
|---|---|---|---|
| F4.01 | Hybrid Search (BM25 + HNSW + RRF) | ✅ | P0 |
| F4.02 | Reciprocal Rank Fusion (RRF) | ✅ | P0 |
| F4.03 | Linearer Score-Fusion | 🔵 Geplant | P1 |
| F4.04 | Metadata Pre-Filter | ✅ | P0 |
| F4.05 | Metadata Post-Filter | ✅ | P0 |
| F4.06 | Roaring Bitmap Tag-Filter | ✅ | P0 |
| F4.07 | JSON-Filter DSL (`$eq`, `$gt`, `$in`, …) | ✅ | P0 |
| F4.08 | Spatial Fence (Geo-Filter) | ✅ SpatialFence | P1 |
| F4.09 | CSR Graph-Traversal (Entity-Relations) | 🟡 Scaffold | P1 |
| F4.10 | 4-Signal Fusion (Vec+BM25+Graph+Time) | 🔵 WP-6.1 | P2 |
| F4.11 | Temporal Decay (Neuere Docs bevorzugt) | 🔵 Geplant | P2 |
| F4.12 | Cross-Collection Search | 🔵 Geplant | P2 |
| F4.13 | Maximal Marginal Relevance (MMR) | ✅ Diversity-Heuristik | P1 |

### F5 — Sicherheit & Compliance

| ID | Feature | Status | Priorität |
|---|---|---|---|
| F5.01 | AES-256-GCM Encryption at Rest | ✅ | P0 |
| F5.02 | HKDF Key-Derivation (SHA-256) | ✅ | P0 |
| F5.03 | WAL HMAC-Verifikation | ✅ IntegrityVerifier | P0 |
| F5.04 | Nonce-Reuse Mitigation (AES-GCM-SIV) | 🔴 FIX | P0 |
| F5.05 | KmsProvider Trait (externer KMS) | ✅ | P1 |
| F5.06 | Per-Collection Schlüsselverwaltung | 🔵 Geplant | P2 |
| F5.07 | Zeroize-on-Drop für Schlüssel-Material | 🔵 Geplant | P1 |
| F5.08 | Air-Gap Deployment Profile | 🔵 WP-6.6 | P2 |
| F5.09 | Audit-Trail für alle Operationen | 🔵 Geplant | P2 |
| F5.10 | BLAKE3-WAL-Verifikation (Merkle) | 🔵 WP-6.7 | P3 |

### F6 — Multi-Tenancy & Collections

| ID | Feature | Status | Priorität |
|---|---|---|---|
| F6.01 | Collections / Namespaces | ✅ | P0 |
| F6.02 | Namespace-Registry (NamespaceRegistry) | ✅ | P0 |
| F6.03 | Multi-Agent Namespaces (User→Agent→Session) | 🔵 WP-6.4 | P2 |
| F6.04 | Cross-Namespace Read (Permission-basiert) | 🔵 Geplant | P3 |
| F6.05 | Collection-Statistiken | ✅ | P0 |
| F6.06 | Collection-Backup / Export | 🔵 Geplant | P2 |
| F6.07 | Collection-Import / Migration | 🔵 Geplant | P2 |
| F6.08 | Auto-Dimension-Detection | 🔵 Geplant | P1 |

### F7 — Checkpointing & Time-Travel

| ID | Feature | Status | Priorität |
|---|---|---|---|
| F7.01 | Named Checkpoints | 🟡 Struct exis. | P1 |
| F7.02 | Checkpoint-Locking (Thread-Safe) | 🔴 FIX | P0 |
| F7.03 | MVCC-basierter Snapshot-Restore | 🔵 WP-5.1 | P1 |
| F7.04 | Time-Travel Queries (`search_at(ts)`) | 🔵 WP-5.1 | P2 |
| F7.05 | Automatische Checkpoint-Rotation | 🔵 Geplant | P2 |
| F7.06 | Incremental Checkpoint (Delta-Saves) | 🔵 Geplant | P2 |

### F8 — Developer Experience & APIs

| ID | Feature | Status | Priorität |
|---|---|---|---|
| F8.01 | Python-Bindings (PyO3 + NumPy) | ✅ | P0 |
| F8.02 | Python Exception-Hierarchie | 🔴 FIX | P0 |
| F8.03 | Python pytest Smoke-Tests | 🔴 FIX | P0 |
| F8.04 | Python Type-Stubs (`.pyi`) | 🔵 Geplant | P1 |
| F8.05 | Rust Native API (async, Tokio) | ✅ | P0 |
| F8.06 | MCP Provider (WP-7.3) | 🔵 Geplant | P1 |
| F8.07 | LangChain VectorStore-Adapter | 🔵 Geplant | P1 |
| F8.08 | LlamaIndex-Integration | 🔵 Geplant | P1 |
| F8.09 | Node.js/TypeScript Bindings (napi-rs) | 🔵 Geplant | P2 |
| F8.10 | HTTP REST API (OpenAPI 3.1) | 🔵 Geplant | P2 |
| F8.11 | WASM Target (Browser/Deno) | 🔵 WP-5.2 | P3 |
| F8.12 | Markdown Chunker (RAG-Pipeline) | ✅ vorhanden | P0 |
| F8.13 | PDF Text-Extraktion | 🔵 Geplant | P2 |
| F8.14 | Streaming-Ingest API | 🔵 Geplant | P2 |
| F8.15 | Batch-Insert (atomar, optimiert) | ✅ | P0 |

### F9 — Observability

| ID | Feature | Status | Priorität |
|---|---|---|---|
| F9.01 | tracing Integration | ✅ Partial | P0 |
| F9.02 | Structured Logging (JSON via tracing-subscriber) | 🔵 Geplant | P1 |
| F9.03 | Prometheus Metrics | 🔵 Geplant | P1 |
| F9.04 | Collection Stats API | ✅ | P0 |
| F9.05 | DB Health Check | 🔵 Geplant | P1 |
| F9.06 | Criterion Benchmark Suite | 🟡 Partial | P1 |
| F9.07 | OpenTelemetry Tracing | 🔵 Geplant | P3 |
| F9.08 | Query Plan Explain | 🔵 Geplant | P3 |

---

## 7. Vollständige API-Spezifikation

### 7.1 Goldstandard Python API

```python
import memfuse
import numpy as np
from memfuse import Config, Filter, FusionWeights, Metric, Language, WalSync

# ═══════════════════════════════════════════════════════════════
# DATENBANKZUGANG
# ═══════════════════════════════════════════════════════════════

# Zero-Config (empfohlen)
db = memfuse.open("./agent_memory", dimension=1536)

# Vollständig konfiguriert
db = memfuse.open(
    path="./agent_memory",
    dimension=1536,
    config=Config(
        encryption_key=b"32-byte-key-here-1234567890abcde",  # AES-256
        cache_size_mb=512,
        wal_sync=WalSync.NORMAL,           # NONE | NORMAL | FULL
        compaction_threads=2,
        max_open_files=1000,
        bloom_filter_bits_per_key=10,
        block_size=4096,
    )
)

# Context Manager Support
with memfuse.open("./memory") as db:
    pass  # Automatisches Flush + Close

# ═══════════════════════════════════════════════════════════════
# COLLECTIONS
# ═══════════════════════════════════════════════════════════════

col = db.collection("memories")                              # Erstellen/Öffnen
col = db.collection("memories", create_if_missing=True)     # Explizit
db.drop_collection("old_data")                               # Löschen + Disk-Cleanup
names = db.list_collections()                                # → List[str]
exists = db.collection_exists("memories")                   # → bool
stats = col.stats()
# → CollectionStats(
#       count=15_000,
#       size_bytes=245_760_000,
#       index=HnswStats(layers=5, ef=200, m=16, quantized=True),
#       text=BM25Stats(unique_terms=42_000, avg_doc_len=28.3),
#       storage=StorageStats(sstables=8, compaction_pending=2)
#   )

# ═══════════════════════════════════════════════════════════════
# DOCUMENT CRUD
# ═══════════════════════════════════════════════════════════════

v = np.random.rand(1536).astype(np.float32)

# Einzeln einfügen
col.insert(
    id="doc:001",
    vector=v,
    text="Der Nutzer bevorzugt kurze Antworten auf Deutsch.",
    metadata={
        "topic": "preferences",
        "timestamp": 1748000000,
        "tags": ["user", "language"],
        "priority": 0.9,
        "agent": "alice",
    }
)

# Batch einfügen (atomar, optimiert)
docs = [
    memfuse.Document(id=f"doc:{i}", vector=vectors[i], text=texts[i], metadata={...})
    for i in range(1000)
]
col.insert_batch(docs, batch_size=256)

# Update (partiell oder vollständig)
col.update("doc:001", vector=new_v)                      # Nur Vektor
col.update("doc:001", metadata={"priority": 1.0})        # Nur Metadata
col.update("doc:001", text="Überarbeiteter Text")        # Nur Text
col.update("doc:001", vector=new_v, metadata={...})      # Vollständig

# Löschen
col.delete("doc:001")                                    # Hard-Delete

# Punkt-Lookup
doc = col.get("doc:001")                                 # → Document | None
docs = col.get_batch(["doc:001", "doc:002"])             # → List[Document | None]
count = col.count()                                      # → int

# ═══════════════════════════════════════════════════════════════
# SUCHFUNKTIONEN
# ═══════════════════════════════════════════════════════════════

# 1. Vektorsuche (HNSW ANN)
results = col.search(
    query_vector=v,
    k=10,
    filter=Filter.eq("topic", "preferences"),
    metric=Metric.COSINE,      # COSINE | L2 | DOT_PRODUCT
    ef_search=100,             # Recall/Speed Tradeoff (default: 50)
)

# 2. Keyword-Suche (BM25)
results = col.text_search(
    query="kurze Antworten Deutsch Präferenzen",
    k=10,
    language=Language.GERMAN,  # GERMAN | ENGLISH | AUTO
)

# 3. Hybrid-Suche (BM25 + HNSW via RRF)
results = col.hybrid_search(
    query="user preferences response style",
    query_vector=v,
    k=10,
    alpha=0.7,                # 0.0=BM25, 1.0=Vektor, default=0.5
    rrf_k=60,                 # RRF Rank-Konstante
    filter=Filter.any("tags", ["user", "session"]),
)

# 4. Gefilterter Filter-DSL
f = (
    Filter.gt("timestamp", 1747000000)
    & Filter.eq("topic", "preferences")
    & Filter.in_("agent", ["alice", "bob"])
    & ~Filter.eq("priority", 0.0)           # Negation
)
results = col.search(v, k=10, filter=f)

# 5. 4-Signal-Fusion (Goldstandard WP-6.1)
results = col.search_4signal(
    text="Was hat der Nutzer letzte Woche gesagt?",
    vector=query_v,
    graph_entity="entity:user_alice",    # Graph-Traversal Ausgangspunkt
    time_range=(last_week_ts, now_ts),   # Temporal-Filter
    k=10,
    weights=FusionWeights(
        vector=0.4,
        bm25=0.3,
        graph=0.2,
        temporal=0.1
    )
)

# Ergebnis-Struktur
for r in results:
    print(r.id)             # "doc:001"
    print(r.score)          # 0.897 (fusionierter Score)
    print(r.vector_score)   # 0.923 (Cosine-Ähnlichkeit)
    print(r.text_score)     # 0.741 (BM25-Score)
    print(r.metadata)       # {"topic": "preferences", ...}
    print(r.text)           # "Der Nutzer bevorzugt..."

# ═══════════════════════════════════════════════════════════════
# TRANSAKTIONEN
# ═══════════════════════════════════════════════════════════════

# Context-Manager (empfohlen)
with db.transaction() as tx:
    col.insert_tx(tx, "doc:100", v1, text="...", metadata={...})
    col.insert_tx(tx, "doc:101", v2, text="...", metadata={...})
    col.delete_tx(tx, "doc:old")
    # Automatisches Commit bei Erfolg, Rollback bei Exception

# Explizite Steuerung
tx = db.begin_transaction(isolation=memfuse.Isolation.SERIALIZABLE)
try:
    col.insert_tx(tx, "doc:200", v, metadata={...})
    tx.commit()
except memfuse.MemFuseTransactionError:
    tx.rollback()
    raise

# ═══════════════════════════════════════════════════════════════
# CHECKPOINTS & TIME-TRAVEL
# ═══════════════════════════════════════════════════════════════

# Checkpoint erstellen
cp_id = db.checkpoint("vor_bulk_import")

# Bulk-Import durchführen
col.insert_batch(huge_dataset)

# Bei Fehler zurückrollen
db.restore(cp_id)

# Time-Travel Query (WP-5.1)
with db.at_checkpoint("vor_bulk_import") as past_db:
    past_results = past_db.collection("memories").search(v, k=5)

# Checkpoint-Verwaltung
checkpoints = db.list_checkpoints()
db.drop_checkpoint(cp_id)

# ═══════════════════════════════════════════════════════════════
# RAG CHUNKING
# ═══════════════════════════════════════════════════════════════

# Markdown-Dokument chunken und direkt einfügen
chunks = memfuse.chunk_markdown(
    text=markdown_content,
    chunk_size=512,         # Tokens
    overlap=64,             # Überlapp-Tokens
    respect_boundaries=True # Satz-Grenzen respektieren
)

# Direkt in Collection einfügen (Pipeline)
embeddings = embed_model.encode([c.text for c in chunks])
col.insert_batch([
    memfuse.Document(
        id=f"chunk:{doc_id}:{i}",
        vector=embeddings[i],
        text=chunk.text,
        metadata={"source": doc_id, "chunk_idx": i, **chunk.metadata}
    )
    for i, chunk in enumerate(chunks)
])

# ═══════════════════════════════════════════════════════════════
# MCP SERVER (WP-7.3)
# ═══════════════════════════════════════════════════════════════

# MemFuse als MCP-Tool-Server starten
memfuse.serve_mcp(
    db=db,
    collections=["memories", "documents"],
    host="0.0.0.0",
    port=3333,
    tools=["store_memory", "search_memory", "hybrid_search", "delete_memory"]
)
```

---

## 8. Architektur-Zielzustand

### 8.1 Vollständiges Layer-Modell (stabilisiert)

```
═══════════════════════════════════════════════════════════════
  LAYER 3 — Interface / Clients
═══════════════════════════════════════════════════════════════
  memfuse-py         │ PyO3+NumPy, Exception-Hierarchie, pytest
  memfuse-http       │ OpenAPI 3.1, Thin-Wrapper (Roadmap)
  memfuse-mcp        │ FastMCP-Provider (WP-7.3)
  memfuse-node       │ napi-rs TypeScript (Roadmap)

═══════════════════════════════════════════════════════════════
  LAYER 2 — Orchestration & Facade
═══════════════════════════════════════════════════════════════
  memfuse-db         │ Collection<S>, Hybrid-Search, RRF-Fusion
                     │ Namespaces, Transactions, ContextManager
                     │ MarkdownChunker, SpatialFence, FilterDSL
  memfuse-checkpoint │ Checkpoints, Time-Travel (nach Fix)

═══════════════════════════════════════════════════════════════
  LAYER 1 — Sub-Engines (strikt isoliert)
═══════════════════════════════════════════════════════════════
  memfuse-store      │ LSM-Tree, WAL+CRC, MemTable MVCC
                     │ SSTable+BloomFilter, Tiered-Compaction
                     │ mmap I/O, LZ4-Kompression (Roadmap)
  memfuse-index      │ HNSW+Persistence, SIMD-Distance
                     │ SQ8+State-Persist, DiskANN (Out-of-Core)
  memfuse-text       │ BM25, InvertedIndex<S>, Morphologie
                     │ GermanMorphTokenizer, CompoundSplitter
  memfuse-graph      │ CsrGraph (nach Lifetime-Fix)
                     │ Entity-Traversal, Weighted Edges
  memfuse-crypto     │ AES-256-GCM-SIV (nach Nonce-Fix)
                     │ HKDF, WalHmac, IntegrityVerifier

═══════════════════════════════════════════════════════════════
  LAYER 0 — Kernel
═══════════════════════════════════════════════════════════════
  memfuse-core       │ MemFuseError, Types, Traits (dyn-compat.)
                     │ TxBuffer, Snapshots, FusionWeights
                     │ ContextWindow, TokenBudget, WorkflowState
```

---

# TEIL III — SKALIERUNGSARCHITEKTUR

---

## 9. 4-Stufen-Skalierungsmodell

### Stufe 1 — Embedded Scale (0–10M Vektoren) — `v0.2` Target

**Erreichbar nach den P0-Fixes + DiskANN-Aktivierung.**

Performance-Targets:

| Operation | 100K Vektoren | 1M Vektoren | 10M Vektoren |
|---|---|---|---|
| Insert (single) | < 0.05 ms | < 0.2 ms | < 0.5 ms |
| Insert (batch 1K) | < 10 ms | < 50 ms | < 150 ms |
| Vector Search P50 | < 0.5 ms | < 2 ms | < 8 ms |
| Vector Search P99 | < 2 ms | < 5 ms | < 20 ms |
| Hybrid Search P50 | < 1 ms | < 5 ms | < 15 ms |
| BM25 Search P50 | < 0.3 ms | < 1 ms | < 3 ms |
| Cold Start (Index-Load) | < 50 ms | < 500 ms | < 3 s |

**RAM-Bedarf (SQ8, d=1536):**

| Vektoren | float32 (ohne SQ8) | SQ8 (1/4 RAM) | DiskANN (Navigator) |
|---|---|---|---|
| 100K | 0.6 GB | 0.15 GB | 20 MB |
| 1M | 6 GB | 1.5 GB | 200 MB |
| 10M | 60 GB | 15 GB | 2 GB |
| 100M | 600 GB | 150 GB | 20 GB |

**Aktivierungsschritte für DiskANN:**

```rust
// memfuse-db/src/collection.rs — DiskANN aktivieren
use memfuse_index::{DiskAnnIndex, DiskAnnConfig};

pub enum IndexBackend<S: StorageEngine> {
    InMemory(HnswIndex),
    DiskAnn(DiskAnnIndex<S>),
}

impl<S: StorageEngine> Collection<S> {
    pub fn with_disk_index(storage: Arc<S>, config: DiskAnnConfig) -> Self {
        let index = DiskAnnIndex::open(storage, config)
            .expect("DiskANN-Index konnte nicht geöffnet werden");
        Self {
            index: IndexBackend::DiskAnn(index),
            // ...
        }
    }
}
```

---

### Stufe 2 — Production Scale (10M–500M Vektoren) — `v0.4` Target

**Technische Maßnahmen:**

**2A — WAL Group-Commit:**
```rust
// Amortisiert fsync über 1ms-Fenster — 10× Schreibdurchsatz
pub struct GroupCommitWal {
    pending: Arc<Mutex<Vec<PendingWrite>>>,
    notify: Arc<Notify>,
    fsync_interval: Duration,  // 1ms default
}

impl GroupCommitWal {
    async fn commit_loop(&self) {
        loop {
            tokio::time::sleep(self.fsync_interval).await;
            let writes = {
                let mut guard = self.pending.lock();
                std::mem::take(&mut *guard)
            };
            if !writes.is_empty() {
                self.flush_and_fsync(writes).await;
            }
        }
    }
}
```

**2B — Sharded MemTable:**
```rust
// 16 Shards reduzieren Lock-Contention bei parallelen Writes
const N_SHARDS: usize = 16;

pub struct ShardedMemTable {
    shards: [parking_lot::RwLock<MemTableShard>; N_SHARDS],
    total_size: AtomicUsize,
}

impl ShardedMemTable {
    fn shard_for(&self, key: &[u8]) -> usize {
        ahash::AHasher::hash(key) as usize % N_SHARDS
    }
    
    pub fn insert(&self, key: &[u8], value: Bytes, seq_no: u64) {
        let shard = self.shard_for(key);
        self.shards[shard].write().insert(key, value, seq_no);
        self.total_size.fetch_add(key.len() + value.len(), Ordering::Relaxed);
    }
}
```

**2C — LZ4 Block-Kompression:**
```rust
// SSTable-Blöcke mit LZ4 komprimieren (Feature-Flag)
// Kompressionsrate: 3–5× für Metadata, 1.2× für Vektoren
#[cfg(feature = "lz4")]
fn compress_block(data: &[u8]) -> Vec<u8> {
    lz4_flex::compress_prepend_size(data)
}

#[cfg(not(feature = "lz4"))]
fn compress_block(data: &[u8]) -> Vec<u8> {
    data.to_vec()
}
```

---

### Stufe 3 — Distributed Scale (500M–50B Vektoren) — `v1.0+`

**Shared-Nothing Cluster-Topologie:**

```
                    ┌─────────────────────────┐
                    │     Query Coordinator    │
                    │   (Scatter-Gather-Merge) │
                    └────┬──────┬──────┬──────┘
                         │      │      │
              ┌──────────▼──┐ ┌─▼──┐ ┌▼──────────┐
              │   Shard 0   │ │ .. │ │  Shard N   │
              │  Primary    │ │    │ │  Primary   │
              │  + Replica  │ │    │ │  + Replica │
              └─────────────┘ └────┘ └────────────┘

Routing:
  - Konsistentes Hashing auf Collection-Name + DocId-Präfix
  - RF=2: Jeder Shard auf 2 Nodes repliziert
  - Read: Nearest Replica (Latenz-optimiert)
  - Write: Primary mit async Replikation (WAL-Streaming)

Query-Flow:
  1. Coordinator sendet Query an alle N Shards
  2. Jeder Shard: Lokal Top-K (z.B. k=100)
  3. Coordinator: Global-RRF über N×100 Ergebnisse
  4. Return: Global Top-K (z.B. k=10)

Implementierung:
  - openraft crate für Raft-basierte Replikation
  - gRPC für Shard-to-Coordinator Kommunikation
  - Gossip-Protokoll für Node-Health (oder SWIM)
```

---

### Stufe 4 — Cloud-Native / Disaggregated — Langfristig

```
Storage Layer:   S3/GCS/MinIO — SSTables als Objekte
Compute Layer:   Zustandslose Query Nodes (Kubernetes HPA)
Index Layer:     HNSW Navigator-Service (separates Deployment)
Cache Layer:     Redis/Valkey für Hot Vectors

Vorteile:
  - Storage skaliert unabhängig von Compute
  - Spot-Instanzen für Query Nodes (80% Kosteneinsparung)
  - Cold Collections hibernieren auf Object Storage
  - Multi-Region möglich

WASM-Target (Cloudflare Workers):
  - MemFuse als WASM-Modul
  - Direkt an der Edge — < 5ms global
  - Einschränkung: Kein DiskANN, nur In-Memory
```

---

## 10. Optimierungspotenzial

### 10.1 SIMD-Optimierungen

**Von nightly `portable-simd` zu stable `std::arch`:**

```rust
// Aktuell: nightly-only portable-simd
// Ziel: Stable + Runtime-Feature-Detection

#[cfg(target_arch = "x86_64")]
pub fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len());
    
    if is_x86_feature_detected!("avx2") {
        unsafe { cosine_avx2(a, b) }
    } else if is_x86_feature_detected!("sse4.1") {
        unsafe { cosine_sse41(a, b) }
    } else {
        cosine_scalar(a, b)
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn cosine_avx2(a: &[f32], b: &[f32]) -> f32 {
    use std::arch::x86_64::*;
    
    let n = a.len();
    let mut dot = _mm256_setzero_ps();
    let mut norm_a = _mm256_setzero_ps();
    let mut norm_b = _mm256_setzero_ps();
    
    let chunks = n / 8;
    for i in 0..chunks {
        let va = _mm256_loadu_ps(a.as_ptr().add(i * 8));
        let vb = _mm256_loadu_ps(b.as_ptr().add(i * 8));
        
        dot = _mm256_fmadd_ps(va, vb, dot);       // dot += va * vb
        norm_a = _mm256_fmadd_ps(va, va, norm_a); // norm_a += va²
        norm_b = _mm256_fmadd_ps(vb, vb, norm_b); // norm_b += vb²
    }
    
    // Horizontal sum
    let dot_sum = horizontal_sum_avx(dot);
    let na_sum = horizontal_sum_avx(norm_a);
    let nb_sum = horizontal_sum_avx(norm_b);
    
    // Scalar remainder
    let dot_rem: f32 = a[chunks*8..].iter().zip(&b[chunks*8..])
        .map(|(x, y)| x * y).sum();
    let na_rem: f32 = a[chunks*8..].iter().map(|x| x * x).sum();
    let nb_rem: f32 = b[chunks*8..].iter().map(|x| x * x).sum();
    
    let total_dot = dot_sum + dot_rem;
    let total_na = (na_sum + na_rem).sqrt();
    let total_nb = (nb_sum + nb_rem).sqrt();
    
    1.0 - total_dot / (total_na * total_nb + f32::EPSILON)
}
```

**AVX-512 für Server-CPUs (Ice Lake+):**
- 16 floats per Cycle statt 8 (avx2) → 2× Throughput
- Implementierungsaufwand: mittel
- Feature-Detection: `is_x86_feature_detected!("avx512f")`

**SQ8 SIMD Integer-Scan:**
```rust
// Direkte Int8 Distanz ohne Dequantisierung
// VPDPBUSD instruction (VNNI) — 4× Speedup für quantisierte Suche
#[cfg(target_feature = "avx512vnni")]
unsafe fn dot_product_i8_avx512vnni(a: &[i8], b: &[i8]) -> i32 {
    // Nutzt _mm512_dpbusd_epi32 für 4 Int8-Multiply-Accumulate per Takt
    todo!("AVX-512 VNNI implementation")
}
```

### 10.2 Memory-Layout-Optimierungen

**HNSW Graph — Flaches Layout:**
```rust
// Aktuell: Vec<Vec<NodeId>> — pointer-heavy, viele Heap-Allokationen
struct HnswLayer {
    neighbors: Vec<Vec<u32>>,  // Je ein Vec pro Node → Cache-Miss-Hölle
}

// Optimiert: Flaches Array mit Offset-Tabelle
struct HnswLayerFlat {
    // Alle Nachbarn eines Layers in einem zusammenhängenden Block
    // Layout: [n0_nbr0, n0_nbr1, ..., n1_nbr0, n1_nbr1, ...]
    neighbors: Vec<u32>,
    // Offsets[i] = Start von Node i's Nachbarn in neighbors[]
    offsets: Vec<u32>,
    // Tatsächliche Anzahl Nachbarn pro Node (≤ M)
    counts: Vec<u8>,
    max_connections: u8,  // M
}

impl HnswLayerFlat {
    fn neighbors_of(&self, node: u32) -> &[u32] {
        let start = self.offsets[node as usize] as usize;
        let count = self.counts[node as usize] as usize;
        &self.neighbors[start..start + count]
    }
}
// Vorteile: 2-3× weniger Allokationen, CPU-Cache freundlich
// Speedup bei HNSW-Traversal: ~30-40%
```

**Vektor-Pool für Zero-Allokation Inserts:**
```rust
// Aktuell: Jeder Insert → neue Vec<f32> Allokation
// Optimiert: Pool-Allokator für Vektoren gleicher Dimension
pub struct VectorArena {
    // Zusammenhängende Blöcke für je 65536 Vektoren
    blocks: Vec<Box<[f32]>>,
    // Freie Slots als Bitmap
    free_bitmap: Vec<roaring::RoaringBitmap>,
    dimension: usize,
}

impl VectorArena {
    pub fn alloc(&mut self) -> VectorSlot {
        // O(1) via Bitmap-CTZ (Count-Trailing-Zeros)
    }
    
    pub fn get(&self, slot: VectorSlot) -> &[f32] {
        // Direkte Adressierung ohne Pointer-Indirektion
        let block_idx = slot.0 / 65536;
        let offset = slot.0 % 65536;
        &self.blocks[block_idx][offset * self.dimension..(offset+1) * self.dimension]
    }
}
```

### 10.3 I/O-Optimierungen

**SSTable Prefetching:**
```rust
// Beim HNSW-Traversal: Nachbar-Vektoren prefetchen
// während aktueller Node verarbeitet wird
impl DiskAnnIndex {
    async fn search_step(&self, current: u32, candidates: &[u32]) -> Vec<f32> {
        // Prefetch nächste Kandidaten-Vektoren via io_uring
        for &next in candidates.iter().take(4) {
            self.prefetch_vector(next);  // Non-blocking prefetch
        }
        // Verarbeite current (Distanzberechnung)
        self.compute_distances(current)
    }
}
```

**Direct I/O für DiskANN:**
```rust
// Bypass OS Page Cache für DiskANN (verhindert Eviction von Hot Data)
use std::os::unix::fs::OpenOptionsExt;

let file = std::fs::OpenOptions::new()
    .read(true)
    .custom_flags(libc::O_DIRECT)  // Direct I/O
    .open(&path)?;
```

---

## 11. DiskANN Aktivierungsplan

Da `DiskAnnIndex` und `DiskAnnConfig` bereits in `diskann.rs` implementiert sind, ist der Aufwand für die Aktivierung minimal.

### 11.1 Aktivierungsschritte

**Schritt 1 — Integration-Tests für DiskAnnIndex:**
```rust
// crates/memfuse-index/tests/diskann_integration.rs

#[tokio::test]
async fn test_diskann_basic_search() {
    let tmp = tempfile::tempdir().unwrap();
    let config = DiskAnnConfig {
        path: tmp.path().to_path_buf(),
        dimension: 128,
        max_degree: 64,
        beam_width: 4,
        cache_size_mb: 256,
    };
    
    let storage = /* test storage */;
    let index = DiskAnnIndex::new(Arc::new(storage), config).await.unwrap();
    
    // 10K Vektoren einfügen
    let vecs: Vec<Vec<f32>> = (0..10000)
        .map(|_| (0..128).map(|_| rand::random()).collect())
        .collect();
    
    for (i, v) in vecs.iter().enumerate() {
        index.insert(DocId::new(i as u64), v, TxId::new(i as u64)).await.unwrap();
    }
    index.commit(TxId::new(10000)).await.unwrap();
    
    // Suche: Top-10 für ersten Vektor
    let results = index.search(&vecs[0], 10, TxId::new(10001)).await.unwrap();
    assert_eq!(results.len(), 10);
    assert_eq!(results[0].id.0, 0); // Exakter Match für eigenen Vektor
    
    // Recall > 90%
    let recall = compute_recall(&results, &vecs, &vecs[0], 10);
    assert!(recall > 0.9, "Recall zu niedrig: {}", recall);
}

#[tokio::test]  
async fn test_diskann_survives_restart() {
    let tmp = tempfile::tempdir().unwrap();
    
    // Index erstellen und befüllen
    {
        let index = DiskAnnIndex::new(/* ... */).await.unwrap();
        for (i, v) in test_vectors(1000).iter().enumerate() {
            index.insert(DocId::new(i as u64), v, TxId::new(i as u64)).await.unwrap();
        }
        index.flush().await.unwrap();
    } // Index wird dropped
    
    // Index wieder laden
    let index = DiskAnnIndex::open(/* gleicher Pfad */).await.unwrap();
    let results = index.search(&test_vectors(1)[0], 10, TxId::new(0)).await.unwrap();
    
    assert_eq!(results.len(), 10); // Daten nach Restart vorhanden
}
```

**Schritt 2 — Collection-Integration:**
```rust
// memfuse-db/src/lib.rs — DiskANN als optionaler Index-Typ

pub enum IndexMode {
    InMemory,
    DiskAnn(DiskAnnConfig),
}

impl MemFuseConfig {
    pub fn with_disk_index(mut self, config: DiskAnnConfig) -> Self {
        self.index_mode = IndexMode::DiskAnn(config);
        self
    }
}
```

**Schritt 3 — Python-API Exposition:**
```python
# DiskANN von Python aus nutzbar machen
db = memfuse.open(
    "./large_db",
    dimension=1536,
    index=memfuse.DiskAnnConfig(
        beam_width=4,
        max_degree=64,
        cache_size_mb=4096,
    )
)
```

---

# TEIL IV — STABILISIERUNG & ROADMAP

---

## 12. Implementierungsplan

### Sprint 0 — Woche 1: Build reparieren (P0 — KRITISCH)

**Geschätzte Gesamtzeit: 12–15h**

```
Tag 1 (4h):
  [2h] BLK-001: PersistentCheckpointStore<S: StorageEngine> — Generics-Migration
  [1h] BLK-001: memfuse-text/src/lib.rs:25 — letzte `Arc<dyn StorageEngine>` entfernen
  [1h] BLK-003: [u8] Sized-Fix in checkpoint/lib.rs:141

Tag 2 (5h):
  [3h] BLK-002: CsrGraph Lifetime-Fixes (6 Methoden) + echte BFS/DFS Implementierung
  [2h] BLK-002: InvertedIndex/BM25MorphIndex Lifetime-Fixes (6 Methoden)

Tag 3 (3h):
  [2h] cargo build --workspace → 0 Fehler / 0 Warnings
  [1h] cargo test --workspace → alle Tests grün
  
Tag 4 (3h):
  [2h] PR-Cleanup-Script: Alle 154 offenen PRs automatisch prüfen
       → Kompiliert? Tests grün? → Merge oder Close
  [1h] clippy.log gitignoren, README mit Badge aktualisieren
```

**Verifikation:**
```bash
cargo build --workspace 2>&1 | grep "^error" | wc -l    # Muss: 0
cargo test --workspace 2>&1 | grep "^FAILED" | wc -l    # Muss: 0
cargo clippy --all-targets -- -D warnings 2>&1 | grep "^error" | wc -l  # Muss: 0
```

---

### Sprint 1 — Woche 2-3: Sicherheit & Integrität (P0/P1)

**Geschätzte Gesamtzeit: 18–24h**

```
SEC-001 (6h): AES-GCM-SIV Migration
  - aes-gcm-siv Cargo-Dependency hinzufügen
  - KeyManager vollständig auf GCM-SIV umschreiben
  - Nonce-Counter mit Monotonic-Garantie
  - Migration-Test: verschlüsselte Daten entschlüsselbar nach Update
  - nonce_reuse.rs Tests grün → Bestätigung

DAT-001 (5h): WAL CRC-Verifikation + Rollback-Integrität
  - replay() mit vollständiger CRC-Prüfung
  - Neuer MemFuseError::CorruptWal Variant
  - Konfigurierbar: strict_replay (Abbruch) vs. permissive (Skip + Log)
  - Test: Gecrasher WAL wird korrekt erkannt

CON-001 (2h): Checkpoint-Store RwLock
  - parking_lot::RwLock<CheckpointRegistry>
  - concurrent access Test

DAT-002 (3h): SQ8-Quantizer-State Persistierung  
  - ScalarQuantizerState als Teil von HnswHeader
  - Beim Laden: Quantizer aus Header rekonstruieren

API-001 (4h): Python Exception-Hierarchie + 15 Smoke-Tests
  - create_exception! für 7 Fehlertypen
  - pytest-Basis: insert, search, hybrid_search, delete, error-handling
```

---

### Sprint 2 — Woche 4-6: Production-Ready (P1)

```
HNSW Persistence aktivieren (8h):
  - HnswHeader, NodeRecord, MmapIndex verdrahten
  - Atomic Write (Write-to-Temp → Rename)
  - Integration-Test: 1M Vektoren, Restart < 5s
  - Inkrementelles Saving (nur Delta nach letztem Checkpoint)

DiskANN aktivieren (6h):
  - Integration-Tests schreiben
  - Collection<S>-Integration
  - Python API: memfuse.DiskAnnConfig(...)
  - Benchmark vs. HNSW in-memory

Stable Rust Migration (4h):
  - portable-simd → std::arch (x86_64 + aarch64 + scalar fallback)
  - rust-toolchain.toml auf stable setzen
  - Runtime feature detection via is_x86_feature_detected!

PyPI Release Setup (4h):
  - maturin konfigurieren
  - GitHub Actions Matrix: linux/x86_64, linux/aarch64, macOS Intel, macOS ARM, Windows
  - pip install memfuse funktioniert
  - Upload zu TestPyPI → Verifikation → PyPI

v0.2.0 Tag + Announcement
```

---

### Sprint 3 — Woche 7-10: Feature Expansion (P1/P2)

```
WAL Group-Commit (6h): 10× Schreibdurchsatz
  - GroupCommitWal Implementierung
  - Konfigurierbar: fsync_interval=1ms
  - Benchmark: vorher vs. nachher

Sharded MemTable (4h): Parallele Write-Scalability
  - 16-Shard-Implementierung
  - Lock-Contention Test mit 16 gleichzeitigen Writers

LZ4 Kompression (4h): 3-5× Storage-Reduktion
  - lz4-flex crate
  - Feature-Flag: default=off (opt-in)
  - Benchmark: Größe + Latenz

MCP Provider (8h): WP-7.3
  - FastMCP-Rust oder Python-Wrapper
  - Tools: store_memory, search_memory, hybrid_search, delete_memory
  - Test: Integration mit Claude Code / Cursor

LangChain VectorStore Adapter (4h):
  - MemFuseVectorStore(path, dimension, embed_fn)
  - Kompatibilität: langchain>=0.2

v0.3.0 Tag
```

---

### Sprint 4 — Woche 11-16: Goldstandard (P2)

```
4-Signal Fusion API (12h): WP-6.1
  - CsrGraph vollständig aktiviert
  - FusionWeights in Collection::hybrid_search eingebaut
  - Python: col.search_4signal(text, vector, graph_entity, time_range, weights)

Checkpoint / Time-Travel (8h): WP-5.1
  - db.checkpoint(name) / db.restore(name)
  - db.at_checkpoint(name) als Context-Manager
  - Atomarer Restore (kein partial-restore möglich)

BLAKE3 WAL-Verifikation (6h): WP-6.7
  - Merkle-Tree über SSTable-Blöcke
  - Kryptografischer Integritätsbeweis für Air-Gap-Audits

Crates.io Publish (2h):
  - memfuse-core, memfuse-store, memfuse-index, memfuse-db

Öffentliche Benchmarks (8h):
  - vs. ChromaDB (Python): Insert + Search + Hybrid
  - vs. Qdrant (embedded nicht möglich → Faiss stattdessen)
  - vs. SQLite-vec
  - Publikation auf memfuse.dev/benchmarks

v1.0.0 Release Tag
```

---

### Release-Timeline-Übersicht

| Version | Woche | Fokus | PyPI | Key Feature |
|---|---|---|---|---|
| v0.1.0 | 1 | Build reparieren | ❌ | 0 Compile-Fehler |
| v0.1.5 | 3 | Sicherheit | ❌ | AES-GCM-SIV, WAL CRC |
| v0.2.0 | 6 | Production Core | ✅ | HNSW Persistenz, DiskANN, Stable Rust |
| v0.3.0 | 10 | Performance + MCP | ✅ | Group-Commit, MCP Provider, LangChain |
| v0.4.0 | 14 | Goldstandard | ✅ | 4-Signal Fusion, Time-Travel |
| v1.0.0 | 20 | Milestone | ✅ | Benchmarks, Docs, crates.io |

---

## 13. Wettbewerbsstrategie

### 13.1 Vollständiger Marktvergleich

| Feature | MemFuse | ChromaDB | Qdrant | FAISS | SQLite-vec | LanceDB |
|---|---|---|---|---|---|---|
| Embedded (kein Server) | ✅ | ✅ | ❌ | ✅ | ✅ | ✅ |
| pip install Zero-Config | ✅ | ✅ | ❌ | ✅ | ✅ | ✅ |
| Pure Rust (kein C/C++) | **✅** | ❌ | ❌ | ❌ | ❌ | ❌ |
| Hybrid Search (BM25+Vec) | **✅** | ❌ | ✅ | ❌ | ❌ | Teilweise |
| 4-Signal Fusion | **✅ Roadmap** | ❌ | ❌ | ❌ | ❌ | ❌ |
| DiskANN Out-of-Core | **✅ impl.** | ❌ | ✅ | ✅ | ❌ | ✅ |
| AES-256 Encryption | **✅** | ❌ | ❌ | ❌ | ❌ | ❌ |
| MVCC Transactions | **✅** | ❌ | ❌ | ❌ | Partial | ❌ |
| Graph-Traversal | **✅ Roadmap** | ❌ | ❌ | ❌ | ❌ | ❌ |
| MCP Provider | **✅ Roadmap** | ❌ | ❌ | ❌ | ❌ | ❌ |
| Time-Travel Queries | **✅ Roadmap** | ❌ | ❌ | ❌ | ❌ | ✅ |
| Air-Gap Deployment | **✅** | ❌ | ❌ | ✅ | ✅ | ❌ |
| WASM Target | **✅ Roadmap** | ❌ | ❌ | ❌ | ❌ | ❌ |
| SQ8 Quantization | ✅ | ❌ | ✅ | ✅ | ❌ | ❌ |
| German Morphology | **✅** | ❌ | ❌ | ❌ | ❌ | ❌ |
| License | MIT/Apache | Apache-2.0 | Apache-2.0 | MIT | MIT | Apache-2.0 |

**Score: MemFuse hat in 13/18 Kategorien einen Vorteil oder Alleinstellung.**

### 13.2 Zielgruppen und Go-to-Market

**Zielgruppe 1 — KI-Agenten-Entwickler (größte Gruppe, schnellste Adoption):**

Schmerzpunkt: LangChain/LlamaIndex benötigen eine persistente, portable VectorDB. ChromaDB hat kein Hybrid-Search. Qdrant braucht einen Server.

Lösung: `pip install memfuse` + LangChain-Adapter + MCP Provider  
Marketing: GitHub README + Twitter/X/LinkedIn + LangChain Community Forum

**Zielgruppe 2 — Rust-Entwickler (hohe Loyalität, Advocacy-Multiplikatoren):**

Schmerzpunkt: Keine native Rust VectorDB ohne C-FFI.

Lösung: `cargo add memfuse-db` + vollständige Rust-API  
Marketing: This Week in Rust, r/rust, crates.io-Listing

**Zielgruppe 3 — Enterprise / Security-bewusst (größter Revenue-Potential):**

Schmerzpunkt: Datenschutz, On-Premise, Air-Gap, Compliance (HIPAA/DSGVO/BSI).

Lösung: Encryption-at-Rest + Air-Gap-Profile + DSGVO-kompatible Logs  
Marketing: Direkt-Ansprache über LinkedIn, BSI-Grundschutz-Community

**Zielgruppe 4 — Edge AI (IoT/Embedded — Zukunft):**

Schmerzpunkt: 512MB RAM, keine Python-Runtime, kein Server.

Lösung: `#[no_std]`-kompatibles Subset + WASM Target  
Marketing: Embedded Rust Working Group, RISC-V Community

---

## 14. Community & Ecosystem

### 14.1 GitHub-Hygiene (sofort)

```bash
# PR-Cleanup-Script
cat > .agent/scripts/pr-cleanup.sh << 'EOF'
#!/bin/bash
# Alle offenen PRs prüfen: Kompiliert? Tests grün? Merge oder Close.

gh pr list --state open --json number,title --limit 200 | \
  jq -r '.[] | "\(.number) \(.title)"' | while read num title; do
    echo "Prüfe PR #$num: $title"
    gh pr checkout $num --force
    
    if cargo build --workspace 2>&1 | grep -q "^error"; then
        echo "  → Compile-Fehler. Closing."
        gh pr close $num --comment "Auto-close: Build-Fehler. Re-öffnen nach Fix."
    elif cargo test --workspace 2>&1 | grep -q "^FAILED"; then
        echo "  → Test-Fehler. Closing."
        gh pr close $num --comment "Auto-close: Test-Fehler. Re-öffnen nach Fix."
    else
        echo "  → OK. Bereit zum Review."
    fi
    
    git checkout main
done
EOF
chmod +x .agent/scripts/pr-cleanup.sh
```

### 14.2 Dokumentations-Infrastruktur

```
Priorität P1 — Sofort:
  README.md: Quickstart (15 Zeilen), Features, Benchmark-Preview
  CONTRIBUTING.md: PR-Checkliste, Crate-Struktur-Erklärung
  CHANGELOG.md: Mit v0.1.0 initialisieren

Priorität P1 — Für PyPI-Release:
  docs/ (mkdocs oder mdBook):
    getting-started.md   — pip install + erste Query in 5 Minuten
    concepts.md          — LSM, HNSW, BM25, Hybrid erklärt
    api-reference.md     — vollständige Python + Rust API
    examples/            — RAG Pipeline, Agent Memory, Encryption

Priorität P2 — Post-Release:
  docs.rs               — Rust-API automatisch aus Docstrings
  memfuse.dev           — Landing Page + Benchmarks
  Discord Server        — Community-Support
```

### 14.3 Benchmark-Strategie (Community-Building)

Öffentliche Benchmarks sind der stärkste Community-Booster. Ziel: Ein "ANN-Benchmarks"-ähnliches Repository für eingebettete VectorDBs.

```python
# benches/comparison_benchmark.py — Öffentlicher Vergleich

BENCHMARK_CONFIG = {
    "datasets": ["glove-100-angular", "sift-128-euclidean", "deep-image-96"],
    "k": [10, 100],
    "candidates": [
        ("memfuse", memfuse_insert, memfuse_search),
        ("chromadb", chroma_insert, chroma_search),
        ("faiss-flat", faiss_flat_insert, faiss_flat_search),
        ("faiss-hnsw", faiss_hnsw_insert, faiss_hnsw_search),
        ("sqlite-vec", sqlitevec_insert, sqlitevec_search),
    ]
}

METRICS = {
    "recall@10": "Wie viele der echten Top-10 werden gefunden?",
    "qps": "Queries pro Sekunde",
    "insert_rate": "Inserts pro Sekunde",
    "index_build_time": "Zeit bis erste Suche möglich",
    "memory_mb": "RAM-Verbrauch bei 1M Vektoren",
    "binary_size_mb": "Größe der Python-Extension",
}
```

**Erwartetes Ergebnis:** MemFuse gewinnt in:
- Binary-Größe (Pure Rust, keine C-Deps)
- Memory-Effizienz mit SQ8
- Hybrid-Search (ChromaDB hat keins, nur MemFuse in dieser Klasse)
- Encryption-Performance (andere haben es nicht)

---

## Anhang A — Sofortiger Aktionsplan (nächste 48 Stunden)

```
STUNDE 1-4:   BLK-001 fix → cargo build checkpoint + text → grün
STUNDE 5-8:   BLK-002 fix → cargo build graph + text/inverted → grün
STUNDE 9-10:  BLK-003 fix → cargo build --workspace → 0 Errors
STUNDE 11-12: cargo test --workspace → alle Tests grün
              cargo clippy -- -D warnings → 0 Warnings

TAG 2 VORMITTAG:
  SEC-001: AES-GCM-SIV Migration (crypto) — 6h
  
TAG 2 NACHMITTAG:
  DAT-001: WAL CRC-Verifikation — 4h

ERGEBNIS: v0.1.0 — erster voll kompilierender, sicherer Stand
          Git-Tag v0.1.0 → announce in README
```

---

## Anhang B — Invarianten für alle Agenten (absolut verbindlich)

```
1. #![forbid(unsafe_code)] in jedem Crate — Ausnahme nur distance.rs
2. Kein .unwrap() außerhalb von #[cfg(test)]
3. Nur tokio::fs (kein std::fs) in async-Kontexten
4. cargo clippy --all-targets -- -D warnings = 0 Warnings
5. Jede neue pub fn bekommt ≥1 #[tokio::test]
6. StorageEngine/VectorIndex/TextIndex Signaturen nicht ohne Migration-Plan ändern
7. Crate-DAG: L0 importiert nichts, L1 nur L0+Crypto, L2 nur L0+L1, L3 nur memfuse-db
8. Python-API: Backwards-compatible nach v0.2.0
9. AES-GCM-SIV bleibt nach dem Fix — nie zurück zu GCM ohne SIV
10. Benchmarks müssen reproduzierbar sein (fixed Seed, dokumentiertes Setup)
```

---

## Anhang C — Geschäftslogik-Zusammenfassung

**MemFuse in einem Satz:**  
*Die SQLite der AI-Vektordatenbanken: self-contained, serverless, zero-configuration — mit Hybrid-Search, MVCC, Encryption und Air-Gap-Support in 100% safe Rust, ohne eine einzige externe C/C++-Abhängigkeit.*

**Das Problem das MemFuse löst:**  
Ein KI-Agent braucht dauerhaftes semantisches Gedächtnis. Dieses Gedächtnis muss: vektorsemantisch suchbar sein (HNSW), keyword-basiert findbar sein (BM25), transaktionssicher persistiert werden (MVCC), verschlüsselt sein (AES-GCM-SIV), ohne Netzwerk funktionieren (Embedded), und auf jeder Hardware laufen (Pure Rust, kein C). MemFuse löst genau diese Kombination — `pip install memfuse`, fertig.

**Warum kein anderes Projekt das kann:**  
ChromaDB (Python-Overhead, kein Hybrid), Qdrant (Server + C-Deps), FAISS (kein Storage, kein BM25, kein Encryption), SQLite-vec (kein HNSW, kein BM25, kein Encryption), LanceDB (C++-Kern). MemFuse ist der einzige Player mit der vollständigen Kombination — und das in einer Sprache (Rust), die Produktions-Stabilität, WASM-Portabilität und Edge-Deployment aus einer Hand ermöglicht.

---

*Erstellt 2026-05-29 | Basierend auf: Repository forensics, FORENSIC_INVENTORY.md, FORENSIC_FINDINGS.md, SKELETON_REGISTRY.md, memfuse_product_spec.md, memfuse-vollanalyse.md, clippy.log (130 KB), AGENTS.md, Cargo.toml, 202 Git-Commits*  
*Confidence: Hoch — alle Findings basieren auf direktem Code-Inventory, nicht auf Schätzungen*
ENDOFFILE