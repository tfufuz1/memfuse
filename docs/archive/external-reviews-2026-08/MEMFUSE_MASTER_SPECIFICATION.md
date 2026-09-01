# MemFuse — Master-Spezifikation & Strategie
## Das Context-OS für LLMs: Ist-Architektur, geplante Erweiterungen, Sicherheits-/Provenienz-Schicht und Roadmap in einem Dokument

> **Zweck dieser Synthese**: Dieses Dokument konsolidiert vier bisher getrennte Spezifikationen —
> (1) `memfuse_interface_spec.md` (Ist-Zustand, 14 Crates), (2) `memfuse_interface_spec_updated.md`
> (Ist-Zustand v2, 15 Crates inkl. bi-temporaler Kanten/Community Detection/PPR), (3)
> `memfuse_v2_optimierungsspezifikation.md` (Performance-/Hardware-Erweiterung zum Context-OS) und (4)
> `MEMFUSE_INTERFACE_SPECIFICATION.md` (Provenienz-, Sicherheits- und Retrieval-Erweiterung) — zu
> **einer** kohärenten, geschichteten Spezifikation. Nichts wird stillschweigend geglättet: Wo Quellen
> sich ergänzen, werden sie zusammengeführt; wo eine Spezifikation die andere korrigiert (z. B. das
> Fehlen eines `ProvenanceRecord`-Typs), gilt die korrigierte, jüngere Aussage.
>
> **Mission**: MemFuse soll die ultimative Ergänzung zum LLM werden — die Schicht, die Arbeit mit
> Sprachmodellen maximal effizient macht: hardware-nah, performant, mit exzellenter, additiv
> erweiterbarer Architektur.

---

## Inhaltsverzeichnis

0. Mission, Leitprinzipien, Gesamtstrategie
1. Architektur-Gesamtüberblick (Ist-Zustand, konsolidiert)
2. `memfuse-core` — Fundamentschicht: Ist-Zustand + alle geplanten Erweiterungen
3. `memfuse-store` — Persistenzschicht: Ist-Zustand + hardware-naher I/O-Pfad
4. `memfuse-index` — Vektor-Index: Ist-Zustand + Disk-resident Vamana + Quantisierung
5. `memfuse-quant` (NEU) — Embedding-Kompressionsschicht
6. `memfuse-text` — Volltextsuche (Ist-Zustand, unverändert)
7. `memfuse-graph` — CSR-Graph: Ist-Zustand (PPR, Community Detection, bi-temporal) + kausale Dimension (MAGMA)
8. `memfuse-checkpoint` — Checkpoint-Koordination (Ist-Zustand)
9. `memfuse-crypto` — Verschlüsselung & Integrität: Ist-Zustand + Verified Forgetting
10. `memfuse-embed` — Embedding & Reranking (Ist-Zustand)
11. `memfuse-kv` (NEU) — Retrieval↔Inferenz-Brücke (KV-Cache-OS)
12. `memfuse-db` — Orchestrator-Facade: Ist-Zustand + Fusion/Folding/Governance/Provenienz/Routing
13. `memfuse-ollama` — LLM-Integration (Ist-Zustand)
14. `memfuse-agent` — Agent-Orchestrierung: Ist-Zustand + Sleep-Cycle, Foresight, Lifecycle-Management
15. `memfuse-mcp` — MCP-Server: Ist-Zustand + Schreibautorisierungs-Gate
16. `memfuse-tauri` / `memfuse-py` — Außengrenzen: Ist-Zustand + strukturierte Fehlercodes
17. Neue Retrieval-Strategie: Pfad-Extraktion (PathRAG-Stil)
18. Konsolidierte Migrations- und Kompatibilitätsmatrix
19. Konsolidiertes Risikoregister
20. Priorisierte Gesamt-Roadmap (Phasen)
21. Quellenregister mit Verifikationsstatus
22. Statistische Gesamtzusammenfassung
23. Bewusst nicht spezifizierte Bereiche

---

## 0. Mission, Leitprinzipien, Gesamtstrategie

### 0.1 Mission

MemFuse soll **die ultimative Ergänzung zum LLM** werden: eine Schicht, die Gedächtnis, Kontext,
Retrieval und — perspektivisch — die Brücke zur Inferenz-Engine selbst so organisiert, dass die
Arbeit mit LLMs maximal effizient wird. Drei Eigenschaften sind dabei nicht verhandelbar:

- **Hardware-nah**: Der Datenpfad (Storage, Index, künftig KV-Cache) nutzt die tatsächlichen
  Fähigkeiten der Zielhardware (SIMD-Dispatch, io_uring/O_DIRECT, Zero-Copy-Mmap) statt generischer
  Abstraktionen, die Latenzbudget verschenken.
- **Performant**: Jede neue Fähigkeit wird an einem konkreten, meist literaturbelegten
  Effizienzgewinn gemessen (Kompressionsfaktor, Kostenreduktion, Recall-Erhalt) — keine Feature-Arbeit
  ohne quantifizierbaren Nutzen.
- **Exzellente Architektur**: Additive Kompatibilität als Grundgesetz (siehe 0.2). Kein Feature
  rechtfertigt einen Bruch der bestehenden Schichtgrenzen oder Trait-Verträge.

### 0.2 Leitprinzip: Additive Kompatibilität

Jede neue Fähigkeit ist **ein neues Trait, eine neue `#[non_exhaustive]`-Enum-Variante oder ein neues
Crate** — keine bestehende Signatur wird brechend verändert (ADR-035-Governance). Trait-Erweiterungen
folgen dem bereits etablierten `GraphIndex`-Muster: neue Methode mit Default-Implementierung, die
`MemFuseError::CapabilityUnsupported` zurückgibt, wenn der Implementor sie nicht überschreibt. Dieses
Muster zieht sich durch **alle** in diesem Dokument beschriebenen Erweiterungen (Provenienz, kausale
Traversierung, Verified Forgetting, Write-Authorization).

Die wenigen bewussten Ausnahmen (echte Breaking Changes) sind explizit in Abschnitt 18 markiert und
tragen jeweils eine Ein-Zeilen-Migration.

### 0.3 Zwei Erweiterungsachsen

Die geplanten Erweiterungen gliedern sich in zwei komplementäre, unabhängig voneinander umsetzbare
Achsen:

| Achse | Leitfrage | Kernkonzepte | Abschnitte |
|---|---|---|---|
| **A — Performance/Hardware** ("Context-OS") | Wie wird MemFuse so schnell und speichereffizient wie möglich, und wie schließt es die Lücke zur Inferenz-Engine? | Governance-Metadaten, RRF-Fusion, Matryoshka/Quantisierung, disk-residenter Vamana-Index, io_uring, KV-Cache-Brücke, SIMD-Laufzeit-Dispatch, Context-Folding | 3–6, 11–14 (Performance-Teile) |
| **B — Sicherheit/Provenienz** ("Mnemonic Sovereignty") | Wie wird nachvollziehbar, wer wann welchen Speichereintrag erzeugt hat, und wie wird verhindert, dass diese Herkunft gefälscht oder vergiftet wird? | `ProvenanceRecord`, kausale Graph-Dimension (MAGMA), kalibriertes Routing, Sleep-Cycle-Konsolidierung, Verified Forgetting, Write-Authorization-Gate | 2.7–2.8, 7.2, 9.2, 12.5–12.6, 14.2–14.3, 15.2 |

Beide Achsen teilen sich dieselbe Fundamentschicht (`memfuse-core`) und denselben additiven
Erweiterungsmechanismus — sie konkurrieren nicht um Architekturentscheidungen, sondern lassen sich
parallel implementieren.

### 0.4 Korrekturen gegenüber älteren Zwischenständen (Ehrlichkeitsprinzip)

Bei der Zusammenführung wurden folgende Abweichungen zwischen den Quelldokumenten aufgelöst — die
jeweils **jüngere, code-verifizierte** Aussage gilt:

| Frühere Behauptung | Korrigierter Stand | Quelle der Korrektur |
|---|---|---|
| "Provenienz-Feld" bei `consolidate_via_llm` bereits vorhanden | `consolidate_via_llm()` trägt Herkunft nur als `source_doc_ids: Vec<DocId>` + Metadata-Flag — **kein** dediziertes `ProvenanceRecord`. Wird in Abschnitt 2.7/12.5 als neuer Typ spezifiziert | Dok. 4, Abschnitt 0 |
| "40 dokumentierte ADRs" | 39 abgeschlossene ADR-Einträge + 1 leeres Template | Dok. 4, Abschnitt 0 |
| Crate-Anzahl "14" (Ist-Dok. v1) | 15 Crates bestätigt in Ist-Dok. v2 (inkl. bi-temporaler Kanten/Community/PPR-Erweiterungen), **17 geplant** nach Hinzunahme von `memfuse-quant` und `memfuse-kv` | Dok. 2 §0.2, Dok. 3 §1 |
| `memfuse-router` "existiert als eigenständiges Modul" | Bestätigt als Teil von `memfuse-db`/`RouterEngine`, nicht als eigenes Crate — Kalibrierung setzt direkt an `RouterEngine::route()` an | Dok. 4 §0, §4 |

---

## 1. Architektur-Gesamtüberblick (Ist-Zustand, konsolidiert)

### 1.1 Schichtenmodell ("Triebwerk / Getriebe") — Ist-Zustand

| Layer | Bezeichnung | Crates (Ist) | + Geplant (additiv) |
|---|---|---|---|
| 0 | Fundament (reine Typen/Traits, kein I/O) | `memfuse-core` | — |
| 1 | Triebwerk (Storage-/Index-Engines) | `memfuse-store`, `memfuse-index`, `memfuse-text`, `memfuse-graph`, `memfuse-crypto`, `memfuse-checkpoint`, `memfuse-embed` | **`memfuse-quant`**, **`memfuse-kv`** |
| 2 | Getriebe (Orchestrator-Facade) | `memfuse-db` | — |
| 3 | Infrastruktur / Zusatzdienste | `memfuse-ollama` | — |
| 4 | Anwendungslogik | `memfuse-agent` | — |
| 5 | Schnittstellen nach außen | `memfuse-mcp`, `memfuse-tauri`, `memfuse-py` | — |

**Invariante (unverändert über alle Ausbaustufen)**: `memfuse-core` ist Dependency-Wurzel — kein I/O,
kein async, kein Netzwerk (`#![deny(unsafe_code)]`). `memfuse-quant` erhält denselben Status (reine
Berechnung, SIMD-Codec-Logik, keine I/O).

### 1.2 Vollständiger Abhängigkeitsgraph (Ist + geplant)

```
memfuse-core            (keine internen Abhängigkeiten — Wurzel)
├── memfuse-crypto       → core
├── memfuse-checkpoint   → core
├── memfuse-embed        → core
├── memfuse-ollama       → core
├── memfuse-text         → core
├── memfuse-quant        → core                          [NEU, Achse A]
├── memfuse-store        → core, crypto                  [+ IoBackend-Abstraktion]
├── memfuse-graph        → core, store                   [+ CausalCsrGraph, Achse B]
├── memfuse-index        → core, graph, quant             [Kante zu quant NEU]
├── memfuse-kv           → core, store, index             [NEU, Achse A]
├── memfuse-db           → core, checkpoint, embed, graph, index, ollama, store, text, kv
│   ├── memfuse-agent    → core, checkpoint, db, graph, store
│   │   └── memfuse-mcp  → agent, core, crypto, db, ollama
│   ├── memfuse-tauri    → core, db, graph, ollama, kv    [Kante zu kv NEU]
│   └── memfuse-py       → core, db
└── xtask                (Workspace-Build-Tool, kein Produktiv-Code)
```

`memfuse-kv` ist opt-in über Feature-Flag `context-cache`, damit reine Storage-Konsumenten
(z. B. `memfuse-py` für Batch-Ingestion) diese Abhängigkeit nicht mitziehen müssen.

### 1.3 Kompilierzeit-Lint-Kontrakte

| Crate | Attribut | Bedeutung |
|---|---|---|
| `memfuse-core` | `#![deny(unsafe_code)]`, `#![warn(missing_docs)]` | Kein unsafe |
| `memfuse-store` | `#![forbid(unsafe_code)]` | Kein unsafe (härter) |
| `memfuse-index` | `#![deny(unsafe_code)]` | Unsafe nur für SIMD-Intrinsics (explizit markiert) |
| `memfuse-quant` | `#![deny(unsafe_code)]` | Unsafe nur für SIMD-Intrinsics (Hamming/Int8), analog zu `memfuse-index` |
| `memfuse-db` | `#![forbid(unsafe_code)]` | Kein unsafe |
| `memfuse-graph` | `#![forbid(unsafe_code)]` | Kein unsafe |

**Sovereign-Core-Doktrin**: Einziger Ort mit produktivem `unsafe`-Code im Ist-Zustand:
`memfuse-index/src/distance.rs` (AVX2/AVX512VNNI-Intrinsics). `memfuse-quant` erweitert diese Doktrin
um denselben, eng begrenzten Ausnahmefall für Kompressions-Kernel.

---

## 2. `memfuse-core` — Fundamentschicht

### 2.1 Ist-Zustand: Fehlertyp

```rust
pub type Result<T> = std::result::Result<T, MemFuseError>;

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum MemFuseError {
    Internal(String), InvalidInput(String), NotFound(String),
    PolicyViolation(String), NamespaceViolation(String),          // → HTTP 403
    Storage(String), Io(#[from] std::io::Error),
    #[non_exhaustive] WalCorruption { offset: u64, reason: String },
    #[non_exhaustive] ChecksumMismatch { path: String, block_id: u64 },
    Transaction(String), TransactionTimeout { tx_id: u64, elapsed_ms: u64 },
    Conflict(String), InvalidSequenceNumber(u64),
    Index(String), HnswConnectivityDegraded { deleted_ratio: f64 }, Text(String),
    MemoryBudgetExceeded { used_mb: u64, limit_mb: u64 },
    CapabilityUnsupported(String, String),   // (methode, begründung) — Default-Trait-Fallback-Muster
    // … (Rest gemäß Ist-Dokument)
}
```

### 2.2 Ist-Zustand: Kern-Traits

| Trait | Zweck | Implementor (Ist) |
|---|---|---|
| `Checkpoint` | Persistenz-Snapshot-Erzeugung | — |
| `CheckpointCoordinator` *(nicht dyn-kompatibel)* | Koordination mehrerer Checkpoint-Quellen | — |
| `Snapshot` | MVCC-Leseversion | — |
| `StorageEngine` | Persistenzschicht-Abstraktion | `LsmStorage` |
| `VectorIndex` | HNSW-Abstraktion | `HnswIndex` (+ geplant: `VamanaIndex`, §4) |
| `TextEmbeddingEngine` | Embedding-Erzeugung | Ollama-Adapter |
| `TextIndex` | BM25/Inverted-Index | — |
| `GraphIndex` | CSR-Graph/Entity-Relation, inkl. `personalized_page_rank`, `traverse_at` | `CsrGraph` (+ geplant: `causal_traverse`, §7.3) |
| `DistanceCalculator` | Vektordistanz | (+ geplant: `active_kernel()`, §14.1) |

**Erweiterungsmuster (bereits etabliert, für alle Erweiterungen normativ)**: Default-Methoden geben bei
fehlender Implementierung `MemFuseError::CapabilityUnsupported` zurück (z. B. `traverse_at`,
`personalized_page_rank`) — additive Erweiterung ohne Bruch bestehender Implementoren.

### 2.3 Geplante Erweiterung: Strukturierter Fehlercode (Achse A, löst Risiko #5)

```rust
/// Stabiler, FFI-tauglicher Diskriminante-Tag für MemFuseError.
/// #[repr(i32)] — sicher über C-ABI (PyO3, JSON-RPC, Tauri-IPC) transportierbar,
/// ohne die Display-Message zu verlieren.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum MemFuseErrorCode {
    Internal = 0, InvalidInput = 1, NotFound = 2, PolicyViolation = 3,
    NamespaceViolation = 4, Storage = 5, Io = 6, WalCorruption = 7,
    ChecksumMismatch = 8, Transaction = 9, TransactionTimeout = 10,
    Conflict = 11, InvalidSequenceNumber = 12, Index = 13, /* … */
}

/// Strukturierte Fehler-Hülle für Außengrenzen (memfuse-mcp/-tauri/-py):
/// trägt Code + Message + Retry-Fähigkeit, statt verlustbehafteter String-Konvertierung.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FfiError { pub code: MemFuseErrorCode, pub message: String, pub retryable: bool }
```

### 2.4 Geplante Erweiterung: Memory-Governance (Achse A, MemCube-inspiriert)

```rust
/// Governance-Metadaten pro gespeicherter Memory-Einheit (Dokument/Entity/Edge).
/// Separater Schlüsselraum `gov:{doc_id}` — additiv, kein Bruch bestehender KV-Pairs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryGovernance {
    pub created_at_tx: TxId,
    pub source: MemorySource,           // { UserTurn, AgentSynthesis, ToolResult, Import(String) }
    pub ttl: Option<Duration>,
    pub decay: DecayPolicy,             // { None, Exponential{half_life}, AccessCountFloor(u32) }
    pub priority: Priority,             // { Low, Normal, High, Pinned } — Pinned: nie automatisch foldbar
    pub access_count: u64,
    pub last_accessed_tx: TxId,
}
impl MemoryGovernance {
    pub fn new(created_at_tx: TxId, source: MemorySource) -> Self;
    pub fn effective_score(&self, now_tx: TxId) -> f32;   // reine Funktion, Entscheidungsbasis für §14.4
    pub fn touch(&mut self, tx: TxId);
}
```

### 2.5 Geplante Erweiterung: Fusion-Strategie (Achse A, löst Skalenproblem RRF vs. WeightedSum)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum FusionStrategy {
    WeightedSum(FusionWeights),                    // bestehend
    ReciprocalRankFusion { k: u32 },                // NEU, Standard k=60 (Cormack et al. 2009)
    RrfWeighted { k: u32, weights: FusionWeights }, // NEU
}
impl Default for FusionStrategy { fn default() -> Self { Self::ReciprocalRankFusion { k: 60 } } }
```

### 2.6 Geplante Erweiterung: Matryoshka-Trunkierung

```rust
impl Embedding {
    /// Verlustarme Dimensions-Trunkierung — nur zulässig bei MRL-trainiertem Modell.
    pub fn truncate(&self, dim: usize) -> Result<Self>;
}
// Ergänzung an TextEmbeddingEngine:
async fn embed_at_budget(&self, text: &str, dim_budget: Option<usize>) -> Result<Vec<f32>>;
fn supports_truncation(&self) -> bool;   // Default: false
fn native_dim(&self) -> usize;
```

### 2.7 Geplante Erweiterung: `ProvenanceRecord` (Achse B — neuer Typ, keine Vertiefung eines bestehenden Feldes)

**Forschungsbezug**: VMG-Primitiv "Provenance Visibility" (arXiv:2604.16548) sowie MemLineage
(arXiv:2605.14421) und Auto-Dreamer (arXiv:2605.20616), das *"provenance-linked source trajectories"*
als Voraussetzung für sicheres Konsolidieren nennt.

```rust
/// Herkunfts-Nachweis für einen einzelnen Memory-Eintrag (Chunk, Entity, Edge).
/// Append-only im LSM-Store unter Präfix `__provenance:`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceRecord {
    pub provenance_id: u64,
    pub target: ProvenanceTarget,             // Chunk(DocId) | Entity(EntityId) | Edge{from,to}
    pub origin: ProvenanceOrigin,
    pub written_at_tx: TxId,
    pub prompt_hash: Option<[u8; 32]>,        // SHA-256 bei LLM-generierten Einträgen
    pub derived_from: Vec<ProvenanceTarget>,
    pub integrity_hmac: [u8; 32],             // HMAC-Kette, WalHmac-kompatibel
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProvenanceOrigin {
    DirectIngestion,
    LlmConsolidation { model: String },
    SleepCycleConsolidation { workflow_run_id: u64 },
    AgentTool { tool_name: String, node_id: String },
    McpWrite { client_id: String },
}

#[async_trait]
pub trait ProvenanceStore: Send + Sync + 'static {
    async fn record_provenance(&self, record: ProvenanceRecord) -> Result<()>;
    /// Rekursive Herkunftskette über `derived_from`, mit Zyklenerkennung.
    async fn provenance_chain(&self, target: &ProvenanceTarget) -> Result<Vec<ProvenanceRecord>> {
        Err(MemFuseError::capability_unsupported("provenance_chain", "…"))   // Default-Muster
    }
    async fn verify_provenance_integrity(&self, provenance_id: u64) -> Result<bool> {
        Err(MemFuseError::capability_unsupported("verify_provenance_integrity", "…"))
    }
}
```

### 2.8 Geplante Erweiterung: `CausalEdge` — vierte Graph-Dimension (Achse B, MAGMA-Vorstufe)

**Forschungsbezug**: MAGMA (arXiv:2601.03236, ACL 2026) — vier **orthogonale** Graphen (semantisch,
temporal, kausal, entitätsbasiert), nicht vier Kantentypen in einem Graphen. MemFuse hat mit
`Edge.valid_from`/`valid_to` bereits die temporale Dimension; `CausalEdge` ist der erste Schritt zur
kausalen Dimension.

```rust
/// Gerichtete kausale Kante — orthogonal zur bestehenden semantischen Edge, NICHT deren Ersatz.
/// Persistiert unter neuem Präfix `__graph:causal:`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CausalEdge {
    pub cause: EntityId,
    pub effect: EntityId,
    pub causal_strength: f32,                       // eigene Skala, NICHT mit Edge.weight kompatibel
    pub inference_method: CausalInferenceMethod,     // LlmInferred{model, confidence} | ExplicitMarker{marker_text}
    pub valid_from: Option<TxId>,
    pub valid_to: Option<TxId>,
}

// Ergänzung an GraphIndex (Default-Muster):
async fn causal_traverse(&self, start: EntityId, max_hops: usize, direction: CausalDirection)
    -> Result<Vec<(EntityId, f32)>> { Err(MemFuseError::capability_unsupported("graph_causal_traverse", "…")) }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CausalDirection { Forward, Backward }   // Backward: "Warum ist X passiert?"-Queries (CausalRAG2)
```

---

## 3. `memfuse-store` — LSM-Tree-Persistenzschicht

### 3.1 Ist-Zustand (unverändert)

`LsmConfig`/`LsmStorage` (Haupt-Implementierung von `StorageEngine`), `MemTable` (In-Memory-Schreibpuffer),
Write-Ahead-Log (`wal.rs`), SSTable-Format (`sstable.rs`), Kompaktierung (`compaction.rs`),
Checkpoint-Guard, Mmap-Reader.

### 3.2 Geplante Erweiterung: `IoBackend`-Abstraktion (Achse A, hardware-naher I/O-Pfad)

**Problem**: `memfuse-store` nutzt `tokio::fs` + `spawn_blocking` — funktional korrekt, aber jeder
Random-Read durchläuft Threadpool-Dispatch + Page-Cache-Copy statt io_uring/O_DIRECT.

```rust
#[async_trait]
pub trait IoBackend: Send + Sync + 'static {
    async fn read_at(&self, fd: &FileHandle, offset: u64, len: u32) -> Result<Bytes>;
    async fn read_at_batch(&self, reqs: &[(FileHandle, u64, u32)]) -> Result<Vec<Bytes>>; // Default: sequentiell; io_uring: gebatchte SQEs
    async fn write_at(&self, fd: &FileHandle, offset: u64, data: &[u8]) -> Result<()>;
}
// Implementoren: TokioBlockingBackend (= Ist-Zustand, Default), IoUringBackend, MmapReadonlyBackend
```

### 3.3 Zero-Copy-Blockcache

`BlockCache` bleibt strukturell gleich (`RwLock<LruCache<(u64,u64), Bytes>>`); bei
`MmapReadonlyBackend` wird `Bytes` direkt aus dem gemappten Speicherbereich referenziert
(`Bytes::from(Arc<Mmap> + Offset/Len)`) statt kopiert — eliminiert die Kernel→Userspace-Kopie für
heiße SSTable-Blöcke vollständig.

---

## 4. `memfuse-index` — Vektor-Index-Engine

### 4.1 Ist-Zustand

`HnswIndex` (Haupt-Implementierung, rein In-Memory), `DiskAnnIndex` (experimentell, Feature-Flag),
SIMD-Distanzberechnung (`distance.rs`, AVX2/AVX512VNNI), Skalar-Quantisierung (`quantize.rs`),
Persistenzformat.

**Grenze des Ist-Zustands**: `HnswIndex` begrenzt die Collection-Größe auf verfügbares RAM.

### 4.2 Geplante Erweiterung: Disk-resident Vamana-Index (Achse A)

**Forschungsbezug**: DiskANN/Vamana (Subramanya et al. 2019, NeurIPS) hält PQ-komprimierte Vektoren im
RAM, Graph + Full-Precision-Vektoren auf SSD — 5–10× mehr Punkte pro Knoten bei vergleichbarer Latenz
gegenüber HNSW. FreshDiskANN (Singh et al. 2021) löst das Streaming-Insert-Problem.

```rust
pub struct VamanaConfig {
    pub degree_bound: u32,              // R, typ. 64–128
    pub search_list_size: u32,          // L, Breite der Beam-Search
    pub alpha: f32,                     // Pruning-Parameter, typ. 1.2
    pub pq_codec: ProductQuantizer,     // In-RAM-Kompressionsschicht (memfuse-quant)
    pub io_backend: Arc<dyn IoBackend>,
}
pub struct VamanaIndex { /* Graph + Full-Precision-Vektoren auf SSD, PQ-Codes im RAM */ }
impl VamanaIndex {
    pub async fn build(config: VamanaConfig, path: PathBuf) -> Result<Self>;
    /// FreshVamana-Muster: Inserts in In-Memory-Delta-Graph, periodischer Merge in SSD-Hauptgraph
    /// (LSM-Prinzip auf Graphstruktur übertragen).
    async fn merge_delta(&self) -> Result<()>;
}
#[async_trait]
impl VectorIndex for VamanaIndex { /* keine Signaturänderung ggü. bestehendem Trait */ }
```

**Auswahlkriterium**: `HnswIndex` für Collections innerhalb `max_ram_mb`; `VamanaIndex` sobald die
Vektormenge diesen Rahmen um mehr als den PQ-Kompressionsfaktor übersteigt. Koexistenz, kein API-Bruch.

### 4.3 Integration mit `memfuse-quant`

```rust
// Ergänzung an VectorIndex (additiv, Default-Impl = Err(PolicyViolation)):
async fn search_rescored(&self, query: &[f32], plan: &dyn Any, k: usize) -> Result<Vec<ScoredDocument>>;
```

---

## 5. `memfuse-quant` (NEU) — Embedding-Kompressionsschicht (Achse A)

**Zweck**: Kapselt Quantisierungs-Codecs als reine, allokationsarme Transformationen. Kein I/O, kein
async — Layer-1-Peer zu `memfuse-embed`, konsumiert von `memfuse-index`.

```rust
pub trait EmbeddingCodec: Send + Sync {
    type Encoded: Send + Sync + Clone;
    fn encode(&self, v: &Embedding) -> Self::Encoded;
    fn encode_batch(&self, vs: &[Embedding]) -> Vec<Self::Encoded> { vs.iter().map(|v| self.encode(v)).collect() }
    fn approx_distance(&self, a: &Self::Encoded, b: &Self::Encoded) -> f32;
    fn codec_id(&self) -> CodecId;   // { ScalarInt8, Binary, ProductQuantization{m,bits} }
    fn compression_ratio(&self) -> f32;   // z. B. 32.0 (Binary), 4.0 (Int8)
}

pub struct ScalarInt8Codec { calibration: Int8Calibration }
pub struct BinaryCodec;                              // Sign-Bit, gepackt für Popcount-Hamming
pub struct ProductQuantizer { m: u8, bits: u8, codebooks: Vec<Vec<Embedding>> }

/// Zweistufige Suche: Shortlist per approx_distance, Rescoring der Top-N per Full-Precision-Distanz.
/// Reduziert Speicher um compression_ratio() bei ~96 % erhaltener Recall-Qualität.
pub struct RescoringSearchPlan<C: EmbeddingCodec> { pub codec: C, pub shortlist_k: usize, pub final_k: usize }
```

---

## 6. `memfuse-text` — Volltextsuchindex (Ist-Zustand, unverändert)

BM25-Scoring (`bm25.rs`), Inverted Index (`inverted.rs`), Tokenisierung, deutsche
Compound-Wort-Zerlegung (`morphology.rs`). Keine geplanten Änderungen in den Quelldokumenten.

---

## 7. `memfuse-graph` — CSR-Graph, PPR, Community Detection & kausale Dimension

### 7.1 Ist-Zustand: `CsrGraph` (erweitert gegenüber Vorspec)

```rust
struct CsrGraph { ... }
  fn insert_entity_direct(&self, entity: Entity) -> Result<()>
  fn insert_edge_direct(&self, from: EntityId, to: EntityId, weight: f32) -> Result<()>
  fn insert_edge_direct_with_validity(&self, from: EntityId, to: EntityId, weight: f32,
                                       valid_from: Option<TxId>, valid_to: Option<TxId>) -> Result<()>  // bi-temporal
  async fn load_from_storage<S: StorageEngine+?Sized>(storage: &S) -> Result<Self>
  fn compact(&self) / async fn compact_async(self: &Arc<Self>) -> Result<()>
  fn pagerank(...) -> ...
  // impl GraphIndex for CsrGraph (inkl. personalized_page_rank via ppr::compute_ppr)
```

**Community Detection** (`community.rs`, ADR-027, Label Propagation, deterministisch,
`CommunityDetectionConfig{max_iterations: 100, seed: 42}`), persistiert unter
`__graph:community:{entity_id}`, genutzt für `same_community_as`-Filterung in
`hybrid_search_with_strategy`.

**Personalized PageRank** (`ppr.rs`, ADR-027, Power-Iteration, `pub(crate) fn compute_ppr`), bounded
durch `max_iterations`, terminiert bei L1-Norm-Konvergenz.

**Session-Branching-DAG** (`session_dag.rs`, unverändert) — Agent-Gedächtnisverzweigung.

### 7.2 Geplante Erweiterung: `CausalCsrGraph` (Achse B, MAGMA-Muster)

```rust
/// Zweite, ORTHOGONALE CSR-Graph-Instanz für die kausale Dimension.
/// Bewusst KEINE Erweiterung von insert_edge_direct — CausalEdge.causal_strength
/// ist semantisch nicht mit Edge.weight austauschbar (MAGMA-Orthogonalitätsprinzip).
pub struct CausalCsrGraph { inner: parking_lot::RwLock<GraphInner>, storage: Option<Arc<dyn StorageEngine>> }
impl CausalCsrGraph {
    pub fn insert_causal_edge_direct(&self, edge: CausalEdge) -> Result<()>;   // Präfix __graph:causal:
    pub fn causal_traverse(&self, start: EntityId, max_hops: usize, direction: CausalDirection) -> Vec<(EntityId, f32)>;
}
```

`CsrGraph::pagerank()`/`ppr::compute_ppr()` sind graphstrukturagnostisch und lassen sich direkt in
`CausalCsrGraph` wiederverwenden — kein architektonischer Bruch, keine Code-Duplikation.

---

## 8. `memfuse-checkpoint` — Generischer Checkpoint-Koordinator (Ist-Zustand, unverändert)

Keine geplanten Änderungen in den Quelldokumenten (Risiko #7 — zwei `CheckpointGuard`/
`StateCheckpoint`-Paare — bleibt als reine Dokumentationsaufgabe offen, siehe Abschnitt 19).

---

## 9. `memfuse-crypto` — Verschlüsselung & Integrität

### 9.1 Ist-Zustand

Schlüsselverwaltung (`crypto.rs`), WAL-spezifische Kryptografie inkl. `WalHmac` (HMAC-SHA256-Kette,
`crypto.rs`, `WalEntrySnapshot` mit `prev_hmac`), Anti-Tamper (`anti_tamper.rs`), `Zeroize`-Ableitungen
für flüchtige Schlüssel.

### 9.2 Geplante Erweiterung: `DeletionProof` / Verified Forgetting (Achse B)

**Forschungsbezug**: Letztes, technisch anspruchsvollstes VMG-Primitiv (arXiv:2604.16548). SCM
(arXiv:2604.20943): *"intentional forgetting … may help address privacy concerns by enabling users to
have specific information pruned"*.

```rust
/// Kryptographischer Nachweis, dass ein Schlüssel in KEINEM aktiven WAL-Segment/SSTable mehr
/// referenziert wird — beweisbar abwesend via Merkle-Struktur, nicht nur "als gelöscht markiert".
pub struct DeletionProof {
    pub target: ProvenanceTarget,
    pub merkle_root_at_deletion: [u8; 32],
    pub absence_proof_path: Vec<[u8; 32]>,
    pub proof_hmac: [u8; 32],             // WalHmac-Kette-Wiederverwendung
    pub verified_at_tx: TxId,
}

pub trait VerifiedForgetting: Send + Sync + 'static {
    fn prove_deletion(&self, target: &ProvenanceTarget) -> Result<DeletionProof> {
        Err(MemFuseError::capability_unsupported("verified_forgetting", "…"))   // technisch anspruchsvollster Punkt, bewusst als letzter Roadmap-Schritt
    }
    fn verify_deletion_proof(&self, proof: &DeletionProof) -> bool { false }
}
```

---

## 10. `memfuse-embed` — Text-Embedding & Reranking (Ist-Zustand, unverändert bzgl. Kernstruktur)

Reranking (`reranker.rs`). Bekannte offene Frage: `CrossEncoderReranker`-Doppeldefinition (Risiko #4,
siehe Abschnitt 19 — nicht in dieser Spezifikation gelöst, erfordert Einzelanalyse der
Feature-Flag-Kombinatorik). Ergänzt um `TextEmbeddingEngine::embed_at_budget`/`supports_truncation`
(siehe 2.6).

---

## 11. `memfuse-kv` (NEU) — Retrieval↔Inferenz-Brücke (Achse A, zentrales Alleinstellungsmerkmal)

**Zweck**: Schließt die im Ist-Zustand komplett fehlende Lücke zwischen „Kontext wurde retrieviert"
(`ScoredDocument`) und „Kontext ist der Inferenz-Engine als Prefix-KV-Cache bekannt" — der eigentliche
Kern eines *Context-OS*, nicht nur einer Vektordatenbank.

**Forschungsbezug**: PagedAttention (Kwon et al. 2023), CacheGen (Liu et al. 2024), ShadowKV/KVSwap/
IMPRESS (CPU/NVMe-Auslagerung mit selektivem Rückladen).

```rust
/// Verhindert Wiederverwendung eines KV-Caches unter falschem Modellkontext (Silent-Corruption-Vektor).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModelFingerprint { pub model_id: String, pub tokenizer_hash: [u8; 32], pub kv_dtype: KvDType }

pub struct KvCacheRef { pub block_ids: Vec<KvBlockId>, pub token_count: u32, pub fingerprint: ModelFingerprint }

#[derive(Debug, Clone, Copy)]
pub enum KvResidency { Gpu, PinnedHost, Nvme }

#[async_trait]
pub trait ContextCacheBridge: Send + Sync + 'static {
    /// Liefert wiederverwendbaren KV-Cache für eine DocId-Sequenz; nur fehlender Suffix wird prefilled.
    async fn compute_or_reuse_prefix(&self, doc_ids: &[DocId], fp: &ModelFingerprint) -> Result<KvCacheRef>;
    async fn register(&self, doc_ids: &[DocId], cache: KvCacheRef) -> Result<()>;
    /// Verdrängt nach Governance-Signal (MemoryGovernance::effective_score) statt reinem LRU.
    async fn evict(&self, policy: EvictionPolicy) -> Result<EvictionReport>;
    async fn migrate(&self, block_ids: &[KvBlockId], target: KvResidency) -> Result<()>;
    fn stats(&self) -> KvCacheStats;
}
```

**Datenfluss**:
```
Collection::search()/hybrid_search() → Vec<ScoredDocument>
        │
        ▼
ContextCacheBridge::compute_or_reuse_prefix(doc_ids, fingerprint)
        │  Cache-Hit: kein Prefill, direkter Decode-Start
        │  Cache-Miss: Prefill nur für neue Dokumente, danach register()
        ▼
Inferenz-Engine (memfuse-ollama oder externer Serving-Stack) erhält KvCacheRef
```

Opt-in über Feature-Flag `context-cache` (siehe 1.2).

---

## 12. `memfuse-db` — Orchestrator-Facade (zentrale Geschäftslogik)

### 12.1 Ist-Zustand

`MemFuse`-Haupt-Facade, `Collection<S: StorageEngine>` (pro-Namespace-Isolation, erweitert um
Community-API), Transaktions-API, Metadaten-Filter (`filter.rs`), Score-Fusion (`fusion.rs`,
Reciprocal-Rank-Fusion für Hybrid-Suche — Ist-Basis für 12.2), Multi-Step-Retrieval
(Query-Rewriting), Chunking, Kontextfenster-Verwaltung, Kontext-Kompaktierung (`compaction.rs`,
DB-Ebene), Reaper (Hintergrund-Aufräumtask).

### 12.2 Geplante Erweiterung: `HybridQuery`-Fusion (Achse A)

```rust
pub struct HybridQuery {
    pub text_query: Option<String>,
    pub vector_query: Option<Vec<f32>>,
    pub graph_start_node: Option<String>,
    pub fusion: FusionStrategy,          // ersetzt fusion_weights, additiv via #[serde(alias="fusion_weights")]
    pub filter: Option<FilterExpr>,
    pub k: usize,
}
fn fuse_rrf(rankings: &[Vec<ScoredDocument>], k: u32) -> Vec<ScoredDocument> {
    // score(d) = Σ_m 1/(k + rank_m(d)); rank_m(d) = ∞ falls d nicht in Ranking m
}
```

### 12.3 Geplante Korrektur: `MetadataFilter` ↔ `FilterExpr` (löst Risiko #1)

```rust
impl TryFrom<FilterExpr> for MetadataFilter {
    type Error = MemFuseError;
    fn try_from(expr: FilterExpr) -> Result<Self> { /* AST → Prädikat */ }
}
```

### 12.4 Geplante Korrektur: Namensraum-Disambiguierung (löst Risiko #2, **einzige echte Breaking Change**)

`memfuse-db::compaction::CompactionStrategy` → **umbenannt zu** `ContextCompactionStrategy`.
`memfuse-store::compaction::CompactionStrategy` (LSM-Kompaktierung) bleibt unverändert.

### 12.5 Geplante Erweiterung: Context-Folding-Policy (Achse A)

```rust
#[async_trait]
pub trait Foldable: Send + Sync {
    async fn fold(&self, tx: TxId, range: SeqRange, summarizer: &dyn TextEmbeddingEngine) -> Result<FoldedSegment>;
    async fn unfold(&self, tx: TxId, segment_id: DocId) -> Result<Vec<DocId>>;   // Drill-Down zu Originalen
}
pub struct FoldedSegment { pub summary_doc_id: DocId, pub original_doc_ids: Vec<DocId>, pub token_savings: u64 }
pub struct ContextCompactionStrategy { pub trigger: FoldTrigger, pub fold_batch_size: usize }
```

### 12.6 Geplante Erweiterung: `ProvenanceRecord`-Integration in `ContextCompactor` (Achse B)

```rust
impl ContextCompactor {
    /// Wie consolidate_via_llm(), erzeugt zusätzlich ProvenanceRecord mit prompt_hash.
    /// Neue Methode statt Signaturänderung — ADR-035-konform additiv.
    pub async fn consolidate_via_llm_with_provenance(
        &self, chunks: &[ContextChunk], ollama: &OllamaClient,
        provenance_store: &dyn ProvenanceStore, tx: TxId,
    ) -> Result<(CompactedContext, ProvenanceRecord)> { /* … */ }
}
```

### 12.7 Geplante Erweiterung: Cache-bewusste Segmenttrennung (Achse A, Prompt-Caching-Ökonomie)

```rust
/// Trennt CompactedContext in cache-stabile (statische) und cache-volatile (dynamische) Segmente,
/// damit memfuse-ollama den statischen Anteil an den Prompt-Anfang stellt (Prefix-Caching-Voraussetzung).
pub struct CacheAwareContext { pub static_segment: Vec<ContextChunk>, pub dynamic_segment: Vec<ContextChunk> }
impl ContextCompactor {
    pub fn partition_by_cache_stability(&self, chunks: Vec<ContextChunk>, stability_threshold_turns: u32) -> CacheAwareContext;
}
```

### 12.8 Geplante Erweiterung: Kalibriertes Kaskaden-Routing (Achse B)

**Forschungsbezug**: UCCI (arXiv:2605.18796) — isotonische Regression kalibriert Token-Margin-
Unsicherheit auf eine Fehlerwahrscheinlichkeit; senkt Inferenzkosten in einem Produktions-Workload um
31 % (95%-CI [27 %, 35 %]) und reduziert ECE von 0,12 auf 0,03. **Diagnose am realen Code**:
`RouterEngine::route()` aggregiert `chunk.relevance` roh mit statischem `1.2×`-Community-Boost — kein
Kalibrierungsschritt.

```rust
pub struct IsotonicCalibrator { breakpoints: Vec<(f32, f32)> }
impl IsotonicCalibrator {
    pub fn fit(samples: &[(f32, bool)]) -> Self;    // Pool-Adjacent-Violators-Algorithmus (PAVA)
    pub fn calibrate(&self, raw_score: f32) -> f32;
}
pub struct CascadeCostConfig { pub cost_small_profile: f32, pub cost_escalated_profile: f32, pub cost_of_error: f32 }
impl RouterEngine {
    pub async fn route_calibrated(&self, query_embedding: &[f32], query_text: &str,
        calibrator: &IsotonicCalibrator, cost_config: &CascadeCostConfig) -> Result<RoutingDecision>;
}
```

**Einschränkung (Ehrlichkeitsprinzip)**: Die Kalibrierungskurve benötigt ein Hold-out-Set mit bekannten
Korrektheits-Labels — ein Feedback-Signal, das MemFuse aktuell nicht besitzt. Ohne dieses Signal bleibt
`fit()` unbenutzbar; das ist eine **Voraussetzung**, kein automatisches Feature (siehe Roadmap-Phase 4).

---

## 13. `memfuse-ollama` — LLM-Integrationsschicht (Ist-Zustand, unverändert)

Client (`client.rs`), Embedding-Adapter (implementiert `TextEmbeddingEngine`), Modell-Metadaten,
Kontext-Präfixierung. Konsumiert `memfuse-kv` optional (Feature-Flag `context-cache`, siehe 11).

---

## 14. `memfuse-agent` — Agent-Orchestrierung

### 14.1 Ist-Zustand + geplante Erweiterung: SIMD-Laufzeit-Dispatch (Achse A)

Orchestrator (`engine.rs`), Werkzeug-Schnittstelle (`step.rs`), Workflow-Graph (State-Machine),
Audit-Log. Geplant:

```rust
// Ergänzung an DistanceCalculator:
fn active_kernel(&self) -> SimdKernel;   // { Scalar, Avx2, Avx512Vnni } — Introspektion
impl DistanceMetric {
    pub fn best_available() -> SimdKernel;   // std::is_x86_feature_detected!, garantierter Scalar-Fallback
}
```

### 14.2 Geplante Erweiterung: Sleep-Cycle-Konsolidierung (Achse B)

**Forschungsbezug**: Auto-Dreamer (arXiv:2605.20616) — *"region rewriting"*: ein gelernter,
werkzeugnutzender Konsolidator ersetzt einen Speicherbereich vollständig durch eine kompaktere,
neu synthetisierte Version. SCM (arXiv:2604.20943): NREM-artige Verstärkung, REM-artiges "Dreaming",
`ForgettingModule` mit mehrdimensionalem Wichtigkeits-Score (90,9 % Rausch-Reduktion, Benchmark-Zahl).

```rust
pub struct SleepCycleConsolidator { ollama: Arc<OllamaClient>, provenance_store: Arc<dyn ProvenanceStore> }
#[async_trait::async_trait]
impl AgentTool for SleepCycleConsolidator {
    fn name(&self) -> &str { "sleep_cycle_consolidator" }
    /// 1. Liest ContextChunks der Working Region (read-only).
    /// 2. Prüft provenance_chain() — konsolidiert NUR Chunks mit verifizierbarer Herkunft
    ///    (unverifizierte werden übersprungen, "skipped_unverified" im StepResult).
    /// 3. Synthetisiert Ersatz via consolidate_via_llm_with_provenance().
    /// 4. Markiert Original-Chunks als tombstoned (Löschung erst nach Verified-Forgetting-Nachweis, §9.2).
    async fn execute(&self, ctx: &AgentContext, input: serde_json::Value) -> Result<StepResult>;
}
```

**Single-Writer-Invariante**: Pro Working Region darf `SleepCycleConsolidator` nur von genau einer
`OrchestratorEngine`-Instanz gleichzeitig laufen — Lock-Eintrag `__agent:sleep_lock:{community_id}`
mit TTL im LSM-Store.

### 14.3 Geplante Erweiterung: Proaktive Foresight-Events (Achse B)

**Forschungsbezug**: EverMemOS (arXiv:2601.02163, `MemCell = (E,F,P,M)` mit `P` = Foresight/Prospection)
und CogniFold (arXiv:2605.13438, kontinuierliches, unaufgefordertes Falten von Ereignisströmen inkl.
automatischer Reaktivierung ruhender Konzepte).

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForesightSignal {
    pub entity_id: EntityId, pub anticipated_topic: String,
    pub validity_from: TxId, pub validity_to: TxId, pub confidence: f32,
}
/// Reaktivierungs-Event bei semantischer Nähe zu lange nicht abgerufenen ("ruhenden") Entities.
/// Erweiterungsstelle: bestehendes EventSource-Trait.
pub struct DormantEntityReactivationSource { graph: Arc<dyn GraphIndex>, dormancy_threshold_tx_delta: u64 }
#[async_trait::async_trait]
impl EventSource for DormantEntityReactivationSource {
    async fn poll(&self) -> Result<Vec<BackgroundEvent>>;   // nutzt personalized_page_rank() als Seed
}
```

**Ehrliche Einordnung**: CogniFold ist ein Forschungspreprint, kein production-battle-tested System.
`DormantEntityReactivationSource` birgt ein Privacy-Risiko (unerwünschtes proaktives Wiederauftauchen
ruhender Themen) und sollte **standardmäßig deaktiviert** sein, nur bei explizitem Opt-in aktiv.

### 14.4 Geplante Erweiterung: `MemoryLifecycleManager` (Achse A)

```rust
#[async_trait]
pub trait MemoryLifecycleManager: Send + Sync {
    /// Decay-/TTL-Durchlauf, priorisiert nach MemoryGovernance::effective_score.
    async fn sweep(&self, tx: TxId, now_tx: TxId) -> Result<LifecycleSweepReport>;
    /// Konsolidierungsvorschlag (Mem0-ADD/UPDATE/NOOP-Muster) — Entscheidung getrennt von Wirkung.
    async fn plan_consolidation(&self, candidates: &[DocId]) -> Result<Vec<ConsolidationAction>>;
    // ConsolidationAction: Keep | Merge(Vec<DocId>) | Supersede{old,new} | Drop
}
pub struct LifecycleSweepReport { pub folded: u64, pub dropped: u64, pub pinned_skipped: u64 }
```

### 14.5 Weitere geplante Korrekturen

- `AuditLog` → generisch über `S: StorageEngine` (löst Risiko #6, ermöglicht Mock-Storage in Tests).
- `SandboxBridge` → Vereinheitlichung von RPITIT auf `#[async_trait]` (löst Risiko #3, konsistent mit
  den sieben anderen Kern-Traits).

---

## 15. `memfuse-mcp` — Model Context Protocol Server

### 15.1 Ist-Zustand

Server-Kern, JSON-RPC-Protokoll (`McpError`), Sandbox (`sandbox.rs`) — `McpSandbox` erlaubt DB-Reads,
DB-Writes und Code-Execution sind opt-in.

### 15.2 Geplante Erweiterung: Schreibautorisierungs-Gate (Achse B)

**Forschungsbezug**: VMG-Primitiv "Write Authorization" (arXiv:2604.16548) und "Non-Malleable,
Origin-Bound Authority" (arXiv:2606.24322): ein Angreifer kann untrusted content in einer Sitzung
speichern, das später eine folgenreiche Aktion in einer **zukünftigen** Sitzung steuert — reine
Schreibvalidierung zum Zeitpunkt des Schreibens reicht nicht; die Autorität muss an die Herkunft
gebunden sein ("origin-bound"). Die Erweiterung fügt eine inhaltliche Prüfung **vor** dem bestehenden
Opt-in-Gate hinzu.

```rust
pub trait WriteAuthorizationGate: Send + Sync + 'static {
    /// Ok(()) nur, wenn origin eine Schreibautorität für target_capability besitzt — z. B. darf ein
    /// MCP-Client Chunks schreiben, aber keine Provenance-Records fälschen, die eine
    /// LlmConsolidation vortäuschen, die nie stattfand.
    fn authorize_write(&self, origin: &ProvenanceOrigin, target_capability: WriteCapability) -> Result<()>;
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteCapability {
    ContentChunk, EntityGraph,
    /// Restriktivste Capability: kein MCP-Client darf per Default LlmConsolidation/
    /// SleepCycleConsolidation vortäuschen — nur die internen Subsysteme selbst dürfen das.
    ProvenanceAssertion,
}
```

---

## 16. `memfuse-tauri` / `memfuse-py` — Außengrenzen

### 16.1 Ist-Zustand

`memfuse-tauri`: Tauri-Commands (`invoke()`), Anwendungszustand, Ingestion-Pipeline, Ollama-Bridge.
`memfuse-py`: PyO3-Wrapper-Typen (`PySearchResult`, `PyCollection`, `PyMemFuse`, Stats-DTOs).

### 16.2 Geplante Erweiterung: Strukturierte Fehlercodes (Achse A, löst Risiko #5)

```rust
// memfuse-tauri: Commands geben FfiError statt String zurück (native Serialize-Serialisierung).
type TauriResult<T> = std::result::Result<T, FfiError>;

// memfuse-py: expliziter From-Impl statt impliziter Display-Konvertierung.
impl From<MemFuseError> for PyErr {
    fn from(e: MemFuseError) -> PyErr {
        // Code + Message + retryable als strukturiertes Exception-Args-Tupel,
        // sodass `except MemFuseException as e: e.args[0]` den Diskriminanten liefert.
    }
}

// memfuse-mcp: JsonRpcError.data trägt FfiError als strukturiertes JSON-Objekt.
```

`memfuse-tauri`/`memfuse-py` erhalten zusätzlich eine optionale Kante zu `memfuse-kv` (Tauri) bzw.
bleiben ohne Kante (Py, reine Batch-Ingestion-Nutzung ohne `context-cache`-Feature).

---

## 17. Neue Retrieval-Strategie: Pfad-Extraktion (PathRAG-Stil) (Achse B)

**Forschungsbezug**: PathRAG (indirekt verifiziert über Zitationskontext in arXiv:2602.05665 und
arXiv:2602.05143) und HG-RAG (arXiv:2607.14095). Kernidee: explizite relationale Pfade statt eines
vollständigen Subgraphen oder einer flachen Knoten-Score-Liste — kohärenter für nachgelagerte
LLM-Verarbeitung.

```rust
pub enum GraphTraversalStrategy {
    Hops { max_hops: usize },
    PersonalizedPageRank(PprConfig),
    /// NEU: gibt explizite Pfade zurück, direkt als "A → (Relation) → B → (Relation) → C" formatierbar.
    PathExtraction { max_path_length: usize, top_k_paths: usize },
}
pub struct ExtractedPath { pub nodes: Vec<EntityId>, pub edge_labels: Vec<String>, pub cumulative_score: f32 }

impl CsrGraph {
    /// Nutzt bestehende CSR-Traversal-Infrastruktur, hält bei jedem BFS-Schritt den vollständigen
    /// Pfad statt nur den Score fest, dedupliziert auf top_k_paths nach cumulative_score.
    pub async fn extract_paths(&self, seed_nodes: &[EntityId], max_path_length: usize, top_k_paths: usize)
        -> Result<Vec<ExtractedPath>>;
}
```

**Hinweis**: PathRAG selbst wurde nicht direkt per Volltext-Websuche isoliert verifiziert (siehe
Quellenregister, Abschnitt 21) — vor Implementierung sollte eine dedizierte Primärquellen-Suche
erfolgen.

---

## 18. Konsolidierte Migrations- und Kompatibilitätsmatrix

| Änderung | Kompatibilitätsklasse | Migrationsaufwand Downstream |
|---|---|---|
| `MemFuseErrorCode`, `FfiError` | additiv | keine (neue Typen) |
| `MemoryGovernance` | additiv (neuer Schlüsselraum `gov:*`) | keine (Default: `Priority::Normal`, `DecayPolicy::None`) |
| `FusionStrategy` ersetzt `fusion_weights` | additiv via `#[serde(alias)]` | Default wechselt zu RRF — **Verhaltensänderung dokumentationspflichtig** |
| `memfuse-quant`, `VamanaIndex` | additiv (neuer Index-Typ) | opt-in über `CollectionConfig`; `HnswIndex` bleibt Default |
| `IoBackend` | additiv (`Auto`-Default) | keine; `TokioBlockingBackend` = Ist-Zustand |
| `memfuse-kv` | additiv, eigenes Crate, Feature-Flag `context-cache` | keine für Konsumenten ohne Feature |
| `ContextCompactionStrategy`-Rename | **brechend** (Typname) | einmaliges `sed`-Rename in Downstream-Imports |
| `MetadataFilter: TryFrom<FilterExpr>` | additiv | keine |
| `SandboxBridge` → `#[async_trait]` | **potenziell brechend** bei exotischen Lifetime-Bounds | i. d. R. keine Änderung nötig |
| `AuditLog<S>` generisch | additiv (Default bleibt `LsmStorage`) | nur bei expliziter Typ-Annotation |
| `ProvenanceRecord`/`ProvenanceStore` | additiv (neuer Typ, neues Präfix `__provenance:`) | keine — `consolidate_via_llm_with_provenance()` ist neue Methode neben bestehender |
| `CausalEdge`/`CausalCsrGraph` | additiv (orthogonale Struktur, neues Präfix `__graph:causal:`) | keine |
| `DeletionProof`/`VerifiedForgetting` | additiv (Default-Trait-Fallback) | keine, bis explizit implementiert |
| `WriteAuthorizationGate` | additiv, vorgeschaltet vor bestehendem `McpSandbox`-Opt-in | keine für Clients ohne betroffene Capability |
| `PathExtraction`-Strategie | additiv (neue Enum-Variante) | keine |

---

## 19. Konsolidiertes Risikoregister

| # | Risiko (Ist-Dokument) | Status |
|---|---|---|
| 1 | `MetadataFilter`/`FilterExpr`-Dopplung | **Gelöst**: `TryFrom`-Konvertierungspfad, §12.3 |
| 2 | `CompactionStrategy`-Namenskollision (DB- vs. Store-Ebene) | **Gelöst**: Rename zu `ContextCompactionStrategy`, §12.4 |
| 3 | `SandboxBridge` RPITIT-Inkonsistenz | **Gelöst**: Vereinheitlichung auf `#[async_trait]`, §14.5 |
| 4 | `CrossEncoderReranker`-Doppeldefinition | **Offen** — erfordert Einzelanalyse der Feature-Flag-Kombinatorik in `memfuse-embed/src/reranker.rs` |
| 5 | Fehlervariante geht an Außengrenze verloren | **Gelöst**: `MemFuseErrorCode`/`FfiError`, §2.3, §16.2 |
| 6 | `AuditLog` hart an `LsmStorage` gebunden | **Gelöst**: `AuditLog<S: StorageEngine>`, §14.5 |
| 7 | Zwei `CheckpointGuard`/`StateCheckpoint`-Paare | **Offen** — reine Doku-Klärung (ADR-011), kein API-Fix vorgesehen |
| 8 | `search_filtered`-Default-Error stiller Fallstrick | **Offen** — Empfehlung: Integrationstest pro `VectorIndex`-Implementor (inkl. `VamanaIndex`) |
| B1 | UCCI-Kalibrierung ohne Feedback-Signal | **Blockiert** — Voraussetzung (Korrektheits-Feedback-Signal) fehlt, siehe §12.8 |
| B2 | `DormantEntityReactivationSource` Privacy-Risiko | **Mitigiert durch Design** — standardmäßig deaktiviert, Opt-in erforderlich, §14.3 |
| B3 | `VerifiedForgetting` technisch ungelöst | **Langfristig verfolgt** — bewusst letzter Roadmap-Schritt, §9.2 |
| B4 | PathRAG nicht isoliert primärquellen-verifiziert | **Vor Implementierung nachzuholen** — dedizierte Suche empfohlen, §17 |

---

## 20. Priorisierte Gesamt-Roadmap (Phasen)

Die Reihenfolge kombiniert Umsetzungsrisiko, Abhängigkeitsreihenfolge und Sofortnutzen. Jede Phase ist
für sich releasefähig (additive Kompatibilität, §0.2).

**Phase 1 — Fundament & Quick Wins (geringes Risiko, hoher Sofortnutzen)**
`MemFuseErrorCode`/`FfiError` (§2.3, §16.2) · `MetadataFilter: TryFrom<FilterExpr>` (§12.3) ·
`ContextCompactionStrategy`-Rename (§12.4) · `AuditLog<S>` generisch (§14.5) ·
`SandboxBridge` → `#[async_trait]` (§14.5) · `FusionStrategy`/RRF (§2.5, §12.2).
→ Löst Risiken 1, 2, 3, 5, 6 fast vollständig ohne neue Crates.

**Phase 2 — Speicher- & Recheneffizienz (Achse A, Kern des "hardware-nah")**
`memfuse-quant` inkl. Matryoshka-Trunkierung (§2.6, §5) · `MemoryGovernance` (§2.4) ·
`IoBackend`-Abstraktion inkl. Zero-Copy-Blockcache (§3.2–3.3) · SIMD-Laufzeit-Dispatch (§14.1).
→ Direkter Latenz-/Speichergewinn, keine Abhängigkeit von Provenienz-Arbeit.

**Phase 3 — Skalierung & Context-OS-Kern (Achse A, größter architektonischer Hebel)**
`VamanaIndex` disk-resident (§4.2, benötigt Phase 2 für `ProductQuantizer`) ·
`memfuse-kv`/`ContextCacheBridge` (§11) · Context-Folding (`Foldable`, §12.5) ·
`MemoryLifecycleManager` (§14.4) · Cache-bewusste Segmenttrennung (§12.7).
→ Das eigentliche Alleinstellungsmerkmal "Context-OS" wird hier greifbar.

**Phase 4 — Provenienz & kausale Dimension (Achse B, Fundament für Vertrauenswürdigkeit)**
`ProvenanceRecord`/`ProvenanceStore` (§2.7) · Integration in `ContextCompactor` (§12.6) ·
`CausalEdge`/`CausalCsrGraph` (§2.8, §7.2) · `PathExtraction`-Retrieval (§17, nach vorheriger
PathRAG-Primärquellenverifikation).
→ Voraussetzung für alle folgenden Sicherheits-/Autonomie-Features.

**Phase 5 — Autonome Konsolidierung & proaktives Verhalten (Achse B, höchstes Neuheitsrisiko)**
`SleepCycleConsolidator` (§14.2, benötigt Phase 4 für Provenienz-Filterung) ·
`ForesightSignal`/`DormantEntityReactivationSource` (§14.3, standardmäßig deaktiviert) ·
kalibriertes Routing via `IsotonicCalibrator` (§12.8, **blockiert bis Feedback-Signal existiert** —
paralleles Vorprojekt: Nutzer-Korrektur-Signal einführen).

**Phase 6 — Sicherheits-Härtung & langfristige Garantien (Achse B, höchste technische Reife nötig)**
`WriteAuthorizationGate` (§15.2) · `DeletionProof`/`VerifiedForgetting` (§9.2, explizit als letzter
Schritt vorgesehen — technisch anspruchsvollster Punkt der gesamten Spezifikation).

**Nicht terminiert / kontinuierlich**: Offene Doku-Risiken (#4, #7, #8, Abschnitt 19) werden
begleitend zu jeder Phase abgearbeitet, sobald die jeweils betroffene Komponente ohnehin verändert
wird ("Boy-Scout-Regel"), nicht als eigener Block.

---

## 21. Quellenregister mit Verifikationsstatus

| Kürzel | Titel | arXiv-ID | Status | Relevanz |
|---|---|---|---|---|
| MAGMA | A Multi-Graph based Agentic Memory Architecture for AI Agents | 2601.03236 | ✅ ACL 2026 Main Conference | §2.8, §7.2 |
| EverMemOS | A Self-Organizing Memory OS for Structured Long-Horizon Reasoning | 2601.02163 | ✅ (v2, 9. Jan. 2026) | §14.3 |
| CogniFold | Always-On Proactive Memory via Cognitive Folding | 2605.13438 | ✅ (v4, 5. Aug. 2026) | §14.3 |
| VMG-Survey | A Survey on Long-Term Memory Security in LLM Agents | 2604.16548 | ✅ (MemTensor Shanghai, 17. Apr. 2026) | §2.7, §9.2, §15.2 |
| MemAudit | Post-hoc Auditing of Poisoned Agent Memory | 2605.23723 | ✅ | Hintergrund (unverändert) |
| MemLineage | Lineage-guided enforcement for LLM agent memory | 2605.14421 | ✅ | §2.7 |
| Origin-Bound Authority | Non-Malleable, Origin-Bound Authority | 2606.24322 | ✅ | §15.2 |
| Auto-Dreamer | Learning Offline Memory Consolidation for Language Agents | 2605.20616 | ✅ (20. Mai 2026) | §14.2 |
| SCM | Sleep-Consolidated Memory with Algorithmic Forgetting | 2604.20943 | ✅ (Forschungsvorschau) | §9.2, §14.2 |
| CausalRAG2/HugRAG | Hierarchical Causal Knowledge Graph Design for RAG | 2602.05143 | ✅ (ICML 2026 angenommen) | §2.8, §7.2 |
| UCCI | Calibrated Uncertainty for Cost-Optimal LLM Cascade Routing | 2605.18796 | ✅ (11. Mai 2026, 31 % Kostenreduktion) | §12.8 |
| PathRAG | (in Surveys referenziert) | — | 🟡 Nur indirekt verifiziert | §17 |
| MemGPT | Virtuelle Speicherhierarchie für LLM-Agenten | — | Packer et al. 2023 | §2.4 |
| MemOS/MemCube | Payload+Metadata-Kapsel mit Governance/TTL/Priorität | 2507.03724 | ✅ | §2.4 |
| Mem0 | Extraktions-/Update-Pipeline (ADD/UPDATE/DELETE/NOOP) | 2504.19413 | ✅ | §14.4 |
| A-Mem | Zettelkasten, retroaktive "Memory Evolution" | 2502.12110 | ✅ | §14.4 |
| Zep | Temporaler Knowledge-Graph | 2501.13956 | ✅ | Hintergrund |
| Context-Folding | Turn-Ebene-Kompaktierung ohne Datenverlust | 2510.11967 | ✅ | §12.5 |
| DiskANN/Vamana | Disk-resident ANN | — | Subramanya et al. 2019, NeurIPS | §4.2 |
| FreshDiskANN | Streaming-Insert für DiskANN | 2105.09613 | ✅ | §4.2 |
| AiSAQ | Eliminiert PQ-RAM-Anteil bei DiskANN | 2404.06004 | ✅ | §4.2 |
| PagedAttention | KV-Cache-Paginierung | — | Kwon et al. 2023, SOSP | §11 |
| RRF | Reciprocal Rank Fusion | — | Cormack et al. 2009, SIGIR | §2.5, §12.2 |
| MRL | Matryoshka Representation Learning | — | Kusupati et al. 2022 | §2.6 |

---

## 22. Statistische Gesamtzusammenfassung

| Metrik | Ist-Zustand (v2) | Geplant (voll ausgebaut) |
|---|---|---|
| Anzahl Crates | 15 | **17** (+ `memfuse-quant`, `memfuse-kv`) |
| Kern-Traits (dyn-safe, async) | 8 | **12** (+ `IoBackend`, `ContextCacheBridge`, `Foldable`, `MemoryLifecycleManager`) |
| `VectorIndex`-Implementoren | 1 (`HnswIndex`) | **2** (+ `VamanaIndex`, disk-resident) |
| Graph-Dimensionen (orthogonal) | 2 (semantisch, temporal) | **3** (+ kausal, MAGMA-Vorstufe) |
| Neue Fundament-Typen (`memfuse-core`) | — | `MemFuseErrorCode`, `FfiError`, `MemoryGovernance`, `FusionStrategy`, `ProvenanceRecord`, `CausalEdge` |
| Gelöste Risiken aus Ist-Dokument | 0/8 | **5/8** direkt gelöst (Rest: Einzelanalyse bzw. Doku-Fixes) |
| Kompressionsfaktor Embedding (Binary+Rescoring) | — | bis 32× Speicher/Durchsatz, ~96 % Recall-Erhalt |
| Kostenreduktion kalibriertes Routing (UCCI, referenziert) | — | 31 % (95%-CI [27 %, 35 %]), ECE 0,12→0,03 — **abhängig von noch zu bauendem Feedback-Signal** |

---

## 23. Bewusst nicht spezifizierte Bereiche

- **KV-Cache-Kompression/-Quantisierung, Speculative Decoding auf Inferenz-Engine-Ebene**: Würde
  MemFuse zu einer eigenen Inferenz-Engine machen — außerhalb des Kern-Scopes. MemFuse bietet
  stattdessen die vorgelagerte Rolle als strukturierter Wissens-Cache (`ContextCacheBridge`, §11).
- **UCCI-Kalibrierung ohne Feedback-Signal**: Schnittstelle spezifiziert (§12.8), aber ohne
  Korrektheits-Feedback-Signal nicht sinnvoll nutzbar — Voraussetzung, kein separates Feature.
- **Vollständige Verifizierung der Anthropic-"Auto Dream"-Referenz**: Der Bezug zu Anthropics
  internem "Auto Dream"-Konzept bleibt community-dokumentiert, nicht offiziell publiziert. Das
  Single-Writer-Invariante-Prinzip (§14.2) lehnt sich an dieses Muster an, ohne Anspruch auf
  Übereinstimmung mit der tatsächlichen internen Implementierung.
- **`CrossEncoderReranker`-Doppeldefinition und `CheckpointGuard`/`StateCheckpoint`-Klärung**:
  bewusst als offene Doku-/Einzelanalyse-Aufgaben belassen (Abschnitt 19), nicht in Interface-Form
  gegossen, um keine verfrühte, unfundierte API-Entscheidung zu treffen.
