# MemFuse 2.0 — Optimierungs- und Erweiterungsspezifikation (Context-OS für LLMs)

**Basis:** `memfuse_interface_spec.md` (Ist-Zustand, 14 Crates, ~40k LOC).
**Auftrag:** Recherche-getriebene Erweiterung zum hardware-nahen, hochperformanten Context-OS.
**Methode:** (1) Literatur-/Repo-Recherche zu LLM-Memory-Architekturen, Vektorindizes, KV-Cache-Management, Quantisierung, I/O-Layer. (2) Mapping jedes Befunds auf konkrete, additive Trait-/Typ-Erweiterungen der bestehenden Crate-Struktur — keine Neuentwicklung von Grund auf, sondern chirurgische Erweiterung entlang der bestehenden Schichtgrenzen. (3) Auflösung der in Abschnitt 15.2 des Ist-Dokuments identifizierten 8 Risiken als Teil der Zielarchitektur.
**Leitprinzip:** Additive Kompatibilität. Jede neue Fähigkeit ist ein neues Trait, eine neue Enum-Variante (`#[non_exhaustive]`) oder ein neues Crate — keine bestehende Signatur wird brechend verändert.

---

## 0. Recherchebasis — Befund → Architekturkonsequenz

| # | Forschungslinie | Kernbefund | Primärquellen | Konsequenz für MemFuse |
|---|---|---|---|---|
| R1 | Agentisches LLM-Memory als OS-Ressource | Memory wird zunehmend als First-Class-Scheduling-Ressource behandelt (nicht als reiner Vektorstore): MemGPT (virtuelle Speicherhierarchie), MemOS/MemCube (Payload+Metadata-Kapsel mit Governance/TTL/Priorität), Mem0 (Extraktions-/Update-Pipeline mit ADD/UPDATE/DELETE/NOOP), A-Mem (Zettelkasten, retroaktive „Memory Evolution"), Zep (temporaler Knowledge-Graph) | Packer et al. 2023 (MemGPT); Li et al. 2025 arXiv:2507.03724 (MemOS); Chhikara et al. 2025 arXiv:2504.19413 (Mem0); Xu et al. 2025 arXiv:2502.12110 (A-Mem); Rasmussen et al. 2025 arXiv:2501.13956 (Zep) | MemFuse hat bereits die Storage-Mechanik (WAL/LSM/HNSW/Graph), aber **keine Governance-Schicht** (TTL, Decay, Priorität, Provenienz). Fehlt als First-Class-Typ. → §2.2 |
| R2 | Long-Horizon-Agenten: Kontext-Verdichtung statt Vollhaltung | Context-Folding/Summarization-basiertes Context-Management verhindert Kontextexplosion bei Multi-Turn-Agenten, ohne Rohdaten zu verlieren (Pointer auf Originale bleibt erhalten); hierarchisches Working-Memory (HiAgent) trennt aktive von archivierter Ebene | Sun et al. 2025 arXiv:2510.11967 (Context-Folding); Hu et al. 2025 (HiAgent, ACL); Fang et al. 2025 arXiv:2510.18866 (LightMem) | `memfuse-agent` besitzt keinen Faltungs-/Kompaktierungsmechanismus auf Turn-Ebene, nur generische `CompactionStrategy` (Namenskollision, Risiko #2 im Ist-Dokument). → §7.4 |
| R3 | Disk-resident ANN statt reinem In-Memory-HNSW | DiskANN/Vamana hält PQ-komprimierte Vektoren im RAM, Graph+Full-Precision-Vektoren auf SSD — 5–10× mehr Punkte pro Knoten bei vergleichbarer Latenz gegenüber HNSW; AiSAQ eliminiert sogar den PQ-RAM-Anteil (~10 MB bei Milliarden Punkten); FreshDiskANN/DiskANN++ lösen das Streaming-Insert-Problem (In-Place-Delta statt R zufällige SSD-Writes pro Insert) | Subramanya et al. 2019 (DiskANN, NeurIPS); Singh et al. 2021 arXiv:2105.09613 (FreshDiskANN); Tatsuno et al. 2024 arXiv:2404.06004 (AiSAQ); Ni et al. 2023 arXiv:2310.00402 (DiskANN++) | `memfuse-index::HnswIndex` ist rein In-Memory — begrenzt die Collection-Größe auf verfügbares RAM. Fehlender Tier für „viel größer als RAM". → §5 |
| R4 | KV-Cache als eigenständige, cachebare Ressource | PagedAttention paginiert KV-Cache blockweise (Analogie zu virtuellem Speicher); CacheGen komprimiert KV-Cache für Transport/Streaming; ShadowKV/KVSwap/IMPRESS lagern KV-Cache auf CPU/NVMe aus und laden selektiv nach Relevanz zurück; Retrieval-Systeme und Inferenz-Engines behandeln Kontext bislang getrennt — jeder Retrieval-Treffer erzwingt Neu-Prefill | Kwon et al. 2023 (PagedAttention, SOSP); Liu et al. 2024 (CacheGen, SIGCOMM); Sun et al. 2024 (ShadowKV); dieses Dokument, arXiv:2511.11907 (KVSwap); Chen et al. 2025 USENIX FAST (IMPRESS) | MemFuse hat keinerlei Brücke zwischen retrieviertem Kontext (`ScoredDocument`) und der KV-Cache-Repräsentation der Inferenz-Engine — jeder Retrieval-Hit kostet vollen Prefill. **Größter ungenutzter Hebel für ein „Context-OS"**. → §6 (neues Crate `memfuse-kv`) |
| R5 | Hybrid-Retrieval-Fusion: Rank- statt Score-basiert | Reciprocal Rank Fusion (RRF) ist robust gegenüber inkompatiblen Score-Skalen zwischen Vektor-, Text- und Graph-Retrieval (kein Tuning von Gewichten nötig); mehrere 2025/2026-GraphRAG-Systeme fusionieren Entity-/Chunk-/Relation-Kandidaten explizit per RRF statt gewichteter Summe | Cormack et al. 2009 (RRF, SIGIR); Min et al. 2025 arXiv:2507.03226 (Practical GraphRAG); RouteRAG arXiv:2512.09487 | `HybridQuery.fusion_weights: FusionWeights` legt ausschließlich gewichtete Linearkombination nahe — anfällig für Skalenprobleme zwischen Cosine-Score, BM25-Score, Graph-Traversal-Score. → §7.1 |
| R6 | Embedding-Kompression: Matryoshka + Quantisierung + Rescoring | Matryoshka Representation Learning erlaubt verlustarmes Trunkieren der Embedding-Dimension zur Laufzeit; Binär-/Int8-Quantisierung mit Float32-Rescoring-Zweitstufe erreicht ~96 % Retrieval-Qualität bei 32× weniger Speicher und bis 32× höherem Durchsatz | Kusupati et al. 2022 (MRL); Shakir et al. 2024 (Embedding Quantization, HuggingFace/Sentence-Transformers); Yamada et al. 2021 (Binary Passage Retriever) | `Embedding { data: Vec<f32> }` ist monolithisch — kein Truncation-, kein Quantisierungspfad. `DistanceMetric` rechnet ausschließlich auf `f32`/`u8`-Rohvektoren ohne Zweistufen-Suche. → §3 |
| R7 | Hardware-naher I/O-Pfad: io_uring statt epoll/Blocking-Threadpool | io_uring vermeidet Syscall-Overhead und Kernel/User-Copies durch geteilte Ringpuffer; etablierte Rust-Storage-Engines (sled/rio) bauen den Schreibpfad direkt darauf auf; O_DIRECT umgeht zusätzlich den Page-Cache für vorhersagbare Latenz bei Random-Reads (SSTable-Blockzugriff) | tokio-rs/io-uring (GitHub); spacejam/rio (GitHub, sled-Schreibpfad); Zero-Copy-Pages-Analyse (O_DIRECT/SWIOTLB-Fallstricke) | `memfuse-store` nutzt `tokio::fs` + `spawn_blocking` (Abschnitt 2 im Ist-Dokument) — funktional korrekt, aber nicht hardware-nah: jeder Random-Read durchläuft Threadpool-Dispatch + Page-Cache-Copy. → §4 |
| R8 | Runtime-SIMD-Dispatch statt Compile-Time-Fixierung | Distanzberechnung profitiert von AVX2/AVX-512-VNNI, aber Zielhardware ist zur Compile-Zeit oft unbekannt (Cloud-Fleet-Heterogenität); Standardmuster: Runtime-Feature-Detection mit Fallback-Kernel-Kette | Etablierte Praxis (`std::is_x86_feature_detected!`, `multiversion`-Crate-Ökosystem) | `memfuse-index/src/distance.rs` hat laut Ist-Dokument AVX2/AVX512VNNI-Intrinsics, aber keine erkennbare Laufzeit-Dispatch-Schicht im Trait-Kontrakt (`DistanceCalculator` exponiert keine Kernel-Introspektion). → §8.1 |

---

## 1. Architektur-Delta — Schichtenmodell v2

| Layer | Bezeichnung | Crates (**fett** = neu) | Änderung |
|---|---|---|---|
| 0 | Fundament | `memfuse-core` | + Governance-/Fusion-/Codec-Typen, `MemFuseErrorCode` |
| 1 | Triebwerk | `memfuse-store`, `memfuse-index`, `memfuse-text`, `memfuse-graph`, `memfuse-crypto`, `memfuse-checkpoint`, `memfuse-embed`, **`memfuse-quant`**, **`memfuse-kv`** | `memfuse-store`: `IoBackend`-Abstraktion. `memfuse-index`: `VamanaIndex` (disk-resident). Zwei neue Crates. |
| 2 | Getriebe | `memfuse-db` | `FusionStrategy`, `ContextCompactionStrategy` (Rename), `FilterExpr → MetadataFilter`-Konvertierung |
| 3 | Infrastruktur | `memfuse-ollama` | unverändert; konsumiert `memfuse-kv` optional |
| 4 | Anwendungslogik | `memfuse-agent` | `Foldable`, `MemoryLifecycleManager`, generisches `AuditLog<S>`, `SandboxBridge` → `#[async_trait]` |
| 5 | Außengrenze | `memfuse-mcp`, `memfuse-tauri`, `memfuse-py` | strukturierte Fehlercodes statt `String`/verlustbehafteter `PyResult`-Konvertierung |

### 1.1 Neuer Abhängigkeitsgraph (Delta, nur neue/geänderte Kanten)

```
memfuse-core
├── memfuse-quant       → core                          [NEU]
├── memfuse-store       → core, crypto                  [IoBackend-Trait ergänzt]
├── memfuse-index       → core, graph, quant             [Kante zu quant NEU]
├── memfuse-kv          → core, store, index             [NEU — Retrieval↔Inference-Brücke]
├── memfuse-db          → core, checkpoint, embed, graph, index, ollama, store, text, kv  [Kante zu kv NEU]
│   ├── memfuse-agent   → core, checkpoint, db, graph, store
│   │   └── memfuse-mcp → agent, core, crypto, db, ollama
│   ├── memfuse-tauri   → core, db, graph, ollama, kv    [Kante zu kv NEU]
│   └── memfuse-py      → core, db
```

**Invariante bleibt bestehen:** `memfuse-core` bleibt I/O-freie Wurzel. `memfuse-quant` ist ebenfalls reine Berechnung (keine I/O) — SIMD/Codec-Logik, analog zu `memfuse-index/distance.rs`, daher `#![deny(unsafe_code)]` mit expliziten Ausnahmen für Intrinsics.

---

## 2. `memfuse-core` — Neue Fundament-Typen

### 2.1 Strukturierter Fehlercode (löst Risiko #5 aus Ist-Dokument)

```rust
/// Stabiler, FFI-tauglicher Diskriminante-Tag für MemFuseError.
/// #[repr(i32)] — sicher über C-ABI (PyO3, JSON-RPC, Tauri-IPC) transportierbar,
/// ohne die Display-Message zu verlieren.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum MemFuseErrorCode {
    Internal = 0,
    InvalidInput = 1,
    NotFound = 2,
    PolicyViolation = 3,
    NamespaceViolation = 4,     // FFI-Konsumenten: → HTTP 403 / permission-denied-Äquivalent
    Storage = 5,
    Io = 6,
    WalCorruption = 7,
    ChecksumMismatch = 8,
    Transaction = 9,
    TransactionTimeout = 10,
    Conflict = 11,
    InvalidSequenceNumber = 12,
    Index = 13,
    HnswConnectivityDegraded = 14,
    Text = 15,
    MemoryBudgetExceeded = 16,
    Sandbox = 17,
    Serialization = 18,
    Crypto = 19,
    CheckpointNotFound = 20,
    Cluster = 21,
    ParseError = 22,
}

impl From<&MemFuseError> for MemFuseErrorCode { fn from(e: &MemFuseError) -> Self { /* 1:1-Mapping */ } }

/// Über jede Außengrenze mitgeführtes Tupel — ersetzt reinen String/PyErr-Verlust.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FfiError { pub code: MemFuseErrorCode, pub message: String, pub retryable: bool }
impl From<&MemFuseError> for FfiError { /* code via obigem From, retryable=true nur für Transaction/Conflict/TransactionTimeout */ }
```

**Kontrakt:** `memfuse-mcp`, `memfuse-tauri`, `memfuse-py` MÜSSEN `FfiError` statt `String`/nackter `PyResult`-Konvertierung propagieren (siehe §9).

### 2.2 Memory-Governance (MemCube-inspiriert, R1)

```rust
/// Governance-Metadaten pro gespeicherter Memory-Einheit (Dokument/Entity/Edge).
/// Analog zu MemCube-Metadata (deskriptiv / Governance / Verhalten), aber
/// flach genug, um verlustfrei in bestehende StorageEngine-KV-Pairs zu passen
/// (Serialisierung als separater `gov:{doc_id}`-Schlüsselraum, additiv, kein Bruch).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryGovernance {
    pub created_at_tx: TxId,
    pub source: MemorySource,           // enum { UserTurn, AgentSynthesis, ToolResult, Import(String) } #[non_exhaustive]
    pub ttl: Option<Duration>,
    pub decay: DecayPolicy,             // enum { None, Exponential { half_life: Duration }, AccessCountFloor(u32) } #[non_exhaustive]
    pub priority: Priority,             // enum { Low, Normal, High, Pinned } — Pinned: nie automatisch foldbar/löschbar
    pub access_count: u64,
    pub last_accessed_tx: TxId,
}
impl MemoryGovernance {
    pub fn new(created_at_tx: TxId, source: MemorySource) -> Self;
    /// Reine Funktion, keine I/O — Entscheidungsbasis für MemoryLifecycleManager (§7.4).
    pub fn effective_score(&self, now_tx: TxId) -> f32;
    pub fn touch(&mut self, tx: TxId);   // access_count += 1, last_accessed_tx = tx
}
```

### 2.3 Fusion-Strategie (löst R5)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum FusionStrategy {
    WeightedSum(FusionWeights),                  // bestehend — Score-basiert, erfordert kalibrierte Skalen
    ReciprocalRankFusion { k: u32 },              // NEU — rank(d) über {vector, text, graph}, Standard k=60 (Cormack et al.)
    RrfWeighted { k: u32, weights: FusionWeights }, // NEU — RRF-Score zusätzlich pro Modalität gewichtet
}
impl Default for FusionStrategy { fn default() -> Self { Self::ReciprocalRankFusion { k: 60 } } }
```

`HybridQuery.fusion_weights: FusionWeights` → `HybridQuery.fusion: FusionStrategy` (additiv über `#[serde(alias)]` für Altformat-Kompatibilität beim Deserialisieren gespeicherter Query-Presets).

### 2.4 Embedding-Erweiterung: Matryoshka-Trunkierung (R6)

```rust
impl Embedding {
    /// Verlustarme Dimensions-Trunkierung — nur zulässig wenn das Embedding-Modell
    /// MRL-trainiert wurde (Vertrag: `TextEmbeddingEngine::supports_truncation() -> bool`, §2.5).
    pub fn truncate(&self, dim: usize) -> Result<Self>;   // Err(InvalidInput) falls dim > self.dim() oder Modell nicht MRL-fähig
}
```

### 2.5 `TextEmbeddingEngine`-Erweiterung

```rust
async fn embed_at_budget(&self, text: &str, dim_budget: Option<usize>) -> Result<Vec<f32>>;  // Default: embed() + truncate()
fn supports_truncation(&self) -> bool;                                                        // Default: false
fn native_dim(&self) -> usize;
```

---

## 3. `memfuse-quant` (NEU) — Embedding-Kompressionsschicht (R6)

**Zweck:** Kapselt Quantisierungs-Codecs als reine, allokationsarme Transformationen. Kein I/O, kein async — Layer-1-Peer zu `memfuse-embed`, konsumiert von `memfuse-index`.
**Lint:** `#![deny(unsafe_code)]` (SIMD-Kernel für Hamming-/Int8-Distanz analog zu `memfuse-index/distance.rs`).

```rust
/// Einheitlicher Codec-Kontrakt — jede Kompressionsstufe implementiert dies.
pub trait EmbeddingCodec: Send + Sync {
    type Encoded: Send + Sync + Clone;
    fn encode(&self, v: &Embedding) -> Self::Encoded;
    fn encode_batch(&self, vs: &[Embedding]) -> Vec<Self::Encoded> { vs.iter().map(|v| self.encode(v)).collect() } // Default
    /// Approximative Distanz auf komprimierter Repräsentation (für Shortlist-Phase).
    fn approx_distance(&self, a: &Self::Encoded, b: &Self::Encoded) -> f32;
    fn codec_id(&self) -> CodecId;                        // enum { ScalarInt8, Binary, ProductQuantization { m: u8, bits: u8 } } #[non_exhaustive]
    fn compression_ratio(&self) -> f32;                   // z. B. 32.0 für Binary, 4.0 für Int8
}

pub struct ScalarInt8Codec { calibration: Int8Calibration }         // min/max pro Dimension, aus Kalibrierungsstichprobe
impl ScalarInt8Codec { pub fn calibrate(samples: &[Embedding]) -> Self; }
impl EmbeddingCodec for ScalarInt8Codec { type Encoded = Vec<i8>; /* … */ }

pub struct BinaryCodec;   // Sign-Bit pro Dimension, gepackt in u64-Wörter für Popcount-Hamming-Distanz
impl EmbeddingCodec for BinaryCodec { type Encoded = Vec<u64>; /* … */ }

pub struct ProductQuantizer { m: u8, bits: u8, codebooks: Vec<Vec<Embedding>> }  // m Subvektoren, 2^bits Zentroide je Subvektor
impl ProductQuantizer { pub fn train(samples: &[Embedding], m: u8, bits: u8) -> Result<Self>; }  // k-means je Subraum
impl EmbeddingCodec for ProductQuantizer { type Encoded = Vec<u8>; /* Asymmetric Distance Computation (ADC) via Lookup-Tabelle */ }

/// Zweistufige Suche: Shortlist per approx_distance auf komprimiertem Codec,
/// Rescoring der Top-N per Full-Precision-Distanz. Reduziert Speicher um
/// Faktor `compression_ratio()` bei ~96 % erhaltener Recall-Qualität (Shakir et al. 2024).
pub struct RescoringSearchPlan<C: EmbeddingCodec> { pub codec: C, pub shortlist_k: usize, pub final_k: usize }
impl<C: EmbeddingCodec> RescoringSearchPlan<C> {
    pub fn new(codec: C, shortlist_multiplier: usize, final_k: usize) -> Self;  // shortlist_k = final_k * shortlist_multiplier (Default: 4×)
}
```

**Integrationspunkt:** `VectorIndex::search` (Kern-Trait, §1.2 Ist-Dokument) bleibt unverändert für Implementoren ohne Quantisierung. Neue Default-Methode:

```rust
// Ergänzung an trait VectorIndex (memfuse-core, additiv, Default-Impl = Err(PolicyViolation), analog zum bestehenden _at-Fail-Safe-Muster)
async fn search_rescored(&self, query: &[f32], plan: &dyn Any /* RescoringSearchPlan<_>, erasure via downcast */, k: usize) -> Result<Vec<ScoredDocument>>;
```

---

## 4. `memfuse-store` — Hardware-naher I/O-Pfad (R7)

### 4.1 `IoBackend`-Abstraktion

```rust
/// Austauschbarer I/O-Unterbau für SSTable-Random-Reads und WAL-Appends.
/// Ersetzt die feste Kopplung an `tokio::fs` + `spawn_blocking` (Ist-Zustand, §2 Ist-Dokument)
/// durch eine Strategie-Wahl, ohne StorageEngine-Trait-Kontrakt zu ändern.
#[async_trait]
pub trait IoBackend: Send + Sync + 'static {
    async fn read_at(&self, fd: &FileHandle, offset: u64, len: u32) -> Result<Bytes>;
    async fn read_at_batch(&self, reqs: &[(FileHandle, u64, u32)]) -> Result<Vec<Bytes>>;  // Default: sequentiell; io_uring-Impl: gebatchte SQEs
    async fn write_at(&self, fd: &FileHandle, offset: u64, data: &[u8]) -> Result<()>;
    async fn fsync(&self, fd: &FileHandle) -> Result<()>;
    fn backend_id(&self) -> IoBackendId;   // enum { TokioBlocking, IoUring, MmapReadonly } #[non_exhaustive]
}

pub struct TokioBlockingBackend { /* Ist-Zustand — unverändert als Fallback/Default für Nicht-Linux */ }
pub struct IoUringBackend { ring: /* tokio-uring oder rustix::io_uring Wrapper */ }  // Linux-only, #[cfg(target_os = "linux")]
pub struct MmapReadonlyBackend { /* SSTable-Blöcke read-only gemappt — zero-copy Bytes::from_static-artig via Arc<Mmap> */ }
```

**Konfigurationserweiterung `LsmConfig`:**
```rust
pub struct LsmConfig {
    // … bestehende Felder unverändert …
    pub io_backend: IoBackendSelector,   // NEU — enum { Auto, Explicit(IoBackendId) }, Default: Auto (Runtime-Detektion: io_uring wenn Kernel ≥ 5.11, sonst TokioBlocking)
}
```

**Invariante (neu, analog zum Zero-Panic-Kontrakt):** `IoBackend`-Implementierungen MÜSSEN bei fehlender Kernel-Unterstützung (io_uring nicht verfügbar) zur `new()`-Zeit auf `TokioBlockingBackend` zurückfallen, nicht paniken — Fail-Safe-Muster konsistent mit bestehender Sovereign-Core-Doktrin.

### 4.2 Zero-Copy-Blockcache

`BlockCache` (Ist: `RwLock<LruCache<(u64,u64), Bytes>>`) bleibt strukturell gleich; bei `MmapReadonlyBackend` wird `Bytes` direkt aus dem gemappten Speicherbereich referenziert (`Bytes::from(Arc<Mmap> + Offset/Len)`) statt kopiert — eliminiert die Kernel→Userspace-Kopie für heiße SSTable-Blöcke vollständig.

---

## 5. `memfuse-index` — Disk-resident Vamana-Index (R3)

**Neuer Implementor von `VectorIndex`**, koexistiert mit `HnswIndex` (Ist-Zustand) — Collection wählt Implementor über Konfiguration, kein API-Bruch.

```rust
pub struct VamanaConfig {
    pub degree_bound: u32,              // R in der Literatur, typ. 64–128
    pub search_list_size: u32,          // L, Breite der Beam-Search
    pub alpha: f32,                     // Pruning-Parameter (Robust-Prune, typ. 1.2)
    pub pq_codec: ProductQuantizer,     // In-RAM-Kompressionsschicht für Shortlist-Phase (memfuse-quant)
    pub io_backend: Arc<dyn IoBackend>,
}

pub struct VamanaIndex { /* Graph + Full-Precision-Vektoren auf SSD via io_backend, PQ-Codes im RAM */ }

impl VamanaIndex {
    pub async fn build(config: VamanaConfig, path: PathBuf) -> Result<Self>;
    /// FreshVamana-Musterlösung (Singh et al. 2021) gegen das R-zufällige-SSD-Writes-Problem:
    /// Inserts werden in einen In-Memory-Delta-Graph gepuffert und periodisch per
    /// Merge-Kompaktierung in den SSD-residenten Hauptgraph integriert — analog zum
    /// LSM-Prinzip aus memfuse-store (MemTable → SSTable), hier auf Graphstruktur übertragen.
    async fn merge_delta(&self) -> Result<()>;
}

#[async_trait]
impl VectorIndex for VamanaIndex {
    // Alle Pflichtmethoden aus dem bestehenden Trait — keine Signaturänderung.
    // search(): Beam-Search über PQ-Distanz (RAM) für Kandidaten-Shortlist,
    //           gefolgt von Full-Precision-Rerank via io_backend.read_at() auf die
    //           SSD-residenten Rohvektoren (2-Stufen-Muster aus AiSAQ/AdANNS).
    // insert(): schreibt in Delta-Graph (RAM), kein sofortiger SSD-Random-Write.
}
```

**Rebuild-Trigger-Anpassung:** Der bestehende Trigger „automatischer Rebuild bei >20 % gelöschten Nodes" (§1.2 Ist-Dokument) gilt für `HnswIndex` unverändert; `VamanaIndex` nutzt stattdessen den kontinuierlichen `merge_delta`-Mechanismus — kein Full-Rebuild nötig, konsistent mit dem inkrementellen FreshDiskANN-Ansatz.

**Auswahlkriterium (Dokumentationspflicht in `CollectionConfig`):** `HnswIndex` für Collections, die vollständig in den `max_ram_mb`-Rahmen (aus `LsmConfig`) passen; `VamanaIndex` sobald die geschätzte Vektormenge diesen Rahmen um mehr als den PQ-Kompressionsfaktor übersteigt.

---

## 6. `memfuse-kv` (NEU) — Retrieval↔Inferenz-Brücke (R4, zentrales Alleinstellungsmerkmal)

**Zweck:** Schließt die im Ist-Dokument komplett fehlende Lücke zwischen „Kontext wurde retrieviert" (`ScoredDocument`) und „Kontext ist der Inferenz-Engine als Prefix-KV-Cache bekannt" — der eigentliche Kern eines *Context-OS*, nicht nur einer Vektordatenbank.
**Abhängigkeiten:** `core`, `store` (für persistentes KV-Cache-Offloading auf NVMe analog zu KVSwap/IMPRESS), `index` (Cache-Key = Dokument-Identität + Modell-Fingerprint).

```rust
/// Fingerprint aus (Modell-ID, Tokenizer-Version, Quantisierungsschema) — verhindert
/// Wiederverwendung eines KV-Caches unter falschem Modellkontext (Silent-Corruption-Vektor).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModelFingerprint { pub model_id: String, pub tokenizer_hash: [u8; 32], pub kv_dtype: KvDType }  // enum { F16, Bf16, Int8, Fp8 }

/// Referenz auf einen materialisierten KV-Cache-Block-Satz für eine Dokumentsequenz.
/// Analog zu PagedAttention „Physical Blocks" — hier persistenzfähig gemacht.
pub struct KvCacheRef { pub block_ids: Vec<KvBlockId>, pub token_count: u32, pub fingerprint: ModelFingerprint }

#[derive(Debug, Clone, Copy)]
pub enum KvResidency { Gpu, PinnedHost, Nvme }   // Tier-Zustand eines Blocks (analog Harvest/Aqua-Tiering, §0 R4)

#[async_trait]
pub trait ContextCacheBridge: Send + Sync + 'static {
    /// Kernoperation: liefert einen wiederverwendbaren KV-Cache für eine Sequenz von
    /// DocIds — falls (teilweise) bereits materialisiert, wird nur der fehlende Suffix
    /// prefilled (analog zu vLLM Prefix-Caching, hier über Retrieval-Grenzen hinweg).
    async fn compute_or_reuse_prefix(&self, doc_ids: &[DocId], fp: &ModelFingerprint) -> Result<KvCacheRef>;

    /// Materialisiert einen frisch berechneten KV-Cache-Block-Satz für spätere Wiederverwendung.
    async fn register(&self, doc_ids: &[DocId], cache: KvCacheRef) -> Result<()>;

    /// Verdrängt Blöcke nach Governance-Signal (MemoryGovernance::effective_score, §2.2)
    /// statt reinem LRU — heiße, aber „unwichtige" Kontexte werden bevorzugt verdrängt.
    async fn evict(&self, policy: EvictionPolicy) -> Result<EvictionReport>;  // enum { Lru, GovernanceWeighted, ExplicitBlocks(Vec<KvBlockId>) }

    /// Tier-Migration eines Block-Satzes (z. B. GPU → Nvme bei Idle-Fenster, MORI-Muster).
    async fn migrate(&self, block_ids: &[KvBlockId], target: KvResidency) -> Result<()>;

    fn stats(&self) -> KvCacheStats;   // { hit_rate: f32, resident_tokens_by_tier: HashMap<KvResidency, u64>, evictions_total: u64 }
}
```

**Datenfluss (textuell):**
```
Collection::search()/hybrid_search() → Vec<ScoredDocument>
        │
        ▼
ContextCacheBridge::compute_or_reuse_prefix(doc_ids, fingerprint)
        │  (Cache-Hit-Anteil: kein Prefill, direkter Decode-Start)
        │  (Cache-Miss-Anteil: Prefill nur für neue Dokumente, danach register())
        ▼
Inferenz-Engine (memfuse-ollama oder externer Serving-Stack) erhält KvCacheRef
```

**Kompatibilität:** `memfuse-ollama` und `memfuse-tauri` erhalten optionale Kante zu `memfuse-kv` (§1.1) — Nutzung ist opt-in über Feature-Flag `context-cache`, damit reine Storage-Konsumenten (`memfuse-py` für Batch-Ingestion) diese Abhängigkeit nicht mitziehen.

---

## 7. `memfuse-db` — Getriebeschicht-Erweiterungen

### 7.1 `HybridQuery` — Fusion (R5, §2.3)

```rust
pub struct HybridQuery {
    pub text_query: Option<String>,
    pub vector_query: Option<Vec<f32>>,
    pub graph_start_node: Option<String>,
    pub fusion: FusionStrategy,          // ersetzt fusion_weights: FusionWeights (additiv via #[serde(alias = "fusion_weights")])
    pub filter: Option<FilterExpr>,
    pub k: usize,
}
```

Implementierung der RRF-Variante im Orchestrator (`Collection::hybrid_search`):
```rust
fn fuse_rrf(rankings: &[Vec<ScoredDocument>], k: u32) -> Vec<ScoredDocument> {
    // score(d) = Σ_m 1 / (k + rank_m(d)), rank_m(d) = ∞ falls d nicht in Ranking m enthalten
    // (Cormack et al. 2009; Standardwahl k=60 balanciert Kopf- gegen Long-Tail-Dominanz)
}
```

### 7.2 `MetadataFilter` ↔ `FilterExpr` (löst Risiko #1 aus Ist-Dokument)

```rust
/// FilterExpr (memfuse-core) = deklaratives Query-AST, vom Client konstruiert.
/// MetadataFilter (memfuse-db) = interne, evaluierende Repräsentation (kompilierter Prädikatsbaum).
impl TryFrom<FilterExpr> for MetadataFilter {
    type Error = MemFuseError;
    fn try_from(expr: FilterExpr) -> Result<Self> { /* AST → Prädikat, Err(ParseError) bei unbekannten Operator-Kombinationen */ }
}
```
**Kontraktänderung:** `HybridQueryBuilder::with_filter` nimmt weiterhin `FilterExpr` entgegen; die Konvertierung zu `MetadataFilter` erfolgt intern beim `build()`-Aufruf — beide Typen bleiben bestehen, aber die Beziehung ist jetzt zur Compile-Zeit erzwungen statt implizit dokumentiert.

### 7.3 Namensraum-Disambiguierung (löst Risiko #2 aus Ist-Dokument)

`memfuse-db::compaction::CompactionStrategy` → **umbenannt zu** `ContextCompactionStrategy`. `memfuse-store::compaction::CompactionStrategy` (LSM-Kompaktierung) bleibt unverändert. Kein Wildcard-Import (`use …::compaction::*`) kann beide mehr kollidieren lassen.

### 7.4 Context-Folding-Policy (R2)

```rust
/// Faltet eine zusammenhängende Sequenz alter, niedrig-priorisierter Memory-Einheiten
/// (MemoryGovernance::priority < High) zu einem einzigen Summary-Knoten, OHNE die
/// Rohdaten zu löschen — nur die Sichtbarkeit im aktiven Retrieval-Pfad wird reduziert
/// (analog Context-Folding/HiAgent: Pointer auf Original bleibt für Audit/Drill-Down).
#[async_trait]
pub trait Foldable: Send + Sync {
    async fn fold(&self, tx: TxId, range: SeqRange, summarizer: &dyn TextEmbeddingEngine) -> Result<FoldedSegment>;
    async fn unfold(&self, tx: TxId, segment_id: DocId) -> Result<Vec<DocId>>;  // Drill-Down zu Originalen
}
pub struct FoldedSegment { pub summary_doc_id: DocId, pub original_doc_ids: Vec<DocId>, pub token_savings: u64 }

pub struct ContextCompactionStrategy {   // (umbenannt, §7.3) — jetzt mit Folding-Trigger statt reiner Löschung
    pub trigger: FoldTrigger,             // enum { TokenBudgetExceeded(TokenBudget), AgeThreshold(Duration), Manual } #[non_exhaustive]
    pub fold_batch_size: usize,
}
```

---

## 8. `memfuse-agent` — Anwendungslogik-Erweiterungen

### 8.1 SIMD-Laufzeit-Dispatch am `DistanceCalculator` (R8)

```rust
// Ergänzung an trait DistanceCalculator (memfuse-core)
fn active_kernel(&self) -> SimdKernel;   // enum { Scalar, Avx2, Avx512Vnni } — Introspektion für Diagnose/Benchmarks
```
```rust
impl DistanceMetric {
    /// Wählt zur Laufzeit den schnellsten verfügbaren Kernel (std::is_x86_feature_detected!),
    /// mit garantiertem Scalar-Fallback — kein Kompilierzeit-Feature-Flag pro Deployment-Ziel nötig.
    pub fn best_available() -> SimdKernel;
}
```
Für quantisierte Codes (`memfuse-quant::ProductQuantizer`, `BinaryCodec`) wird zusätzlich ein `compute_i8_vnni`-Pfad ergänzt — AVX-512-VNNI führt Int8-Dot-Products in einer Instruktion aus (4× Durchsatz ggü. skalarem Int8) und ist der natürliche Beschleunigungspfad für die in §3 eingeführte Zweistufen-Suche.

### 8.2 Generisches `AuditLog<S>` (löst Risiko #6)

```rust
// vorher: struct AuditLog { collection: Collection<LsmStorage>, … }
// nachher:
pub struct AuditLog<S: StorageEngine> { collection: Collection<S>, /* … */ }
impl<S: StorageEngine> AuditLog<S> { /* unveränderte Methodensignaturen, jetzt generisch */ }
```
Ermöglicht Mock-`StorageEngine` in Tests — kein Verhaltensunterschied für Produktivpfad (`LsmStorage` bleibt Default-Instanziierung in `memfuse-db`).

### 8.3 `SandboxBridge` → `#[async_trait]` (löst Risiko #3)

`SandboxBridge` wird von RPITIT auf `#[async_trait]` umgestellt — konsistent mit allen anderen sieben Kern-Traits (`dyn_safety`-Testmodul in `memfuse-core` wird um `Option<&dyn SandboxBridge>`-Assertion erweitert). Downstream-Code, der `impl SandboxBridge` generisch nutzt, bleibt kompatibel (async-trait generiert weiterhin einen `async fn`-kompatiblen Aufrufpfad); dynamischer Dispatch wird neu möglich.

### 8.4 `MemoryLifecycleManager` (R1)

```rust
#[async_trait]
pub trait MemoryLifecycleManager: Send + Sync {
    /// Decay-/TTL-Durchlauf — markiert abgelaufene/verfallene Einheiten zur Faltung (§7.4)
    /// oder Löschung, priorisiert nach MemoryGovernance::effective_score.
    async fn sweep(&self, tx: TxId, now_tx: TxId) -> Result<LifecycleSweepReport>;
    /// Konsolidierung mehrerer verwandter Memory-Einheiten zu einer (Mem0-ADD/UPDATE/NOOP-Muster,
    /// A-Mem „Memory Evolution"): Aufrufer liefert Kandidaten, Rückgabe ist Aktionsplan, keine
    /// automatische Ausführung — Trennung von Entscheidung und Wirkung (Audit-Fähigkeit).
    async fn plan_consolidation(&self, candidates: &[DocId]) -> Result<Vec<ConsolidationAction>>;  // enum { Keep, Merge(Vec<DocId>), Supersede{ old: DocId, new: DocId }, Drop }
}
pub struct LifecycleSweepReport { pub folded: u64, pub dropped: u64, pub pinned_skipped: u64 }
```

---

## 9. Außengrenzen (`memfuse-mcp`, `memfuse-tauri`, `memfuse-py`) — Strukturierte Fehler (löst Risiko #5)

```rust
// memfuse-tauri: Tauri-Commands geben statt String jetzt FfiError zurück (Tauri serialisiert
// beliebige Serialize-Typen als Command-Error nativ — kein Funktionsverlust ggü. String).
type TauriResult<T> = std::result::Result<T, FfiError>;

// memfuse-py: expliziter From-Impl statt impliziter Display-Konvertierung.
impl From<MemFuseError> for PyErr {
    fn from(e: MemFuseError) -> PyErr {
        let ffi: FfiError = (&e).into();
        // Code + Message als strukturiertes Python-Exception-Args-Tupel (code, message, retryable),
        // sodass Python-seitig `except MemFuseException as e: e.args[0]` den Diskriminanten liefert
        // statt Message-String-Parsing.
    }
}

// memfuse-mcp: JsonRpcError.data-Feld trägt FfiError als strukturiertes JSON-Objekt
// (JSON-RPC 2.0 erlaubt beliebige `data`-Payload neben `code`/`message`).
```

---

## 10. Migrations- und Kompatibilitätsmatrix

| Änderung | Kompatibilitätsklasse | Migrationsaufwand Downstream |
|---|---|---|
| `MemFuseErrorCode`, `FfiError` | additiv | keine (neue Typen) |
| `MemoryGovernance` | additiv (neuer Schlüsselraum `gov:*`) | keine für Bestandsdaten (Governance-Default: `Priority::Normal`, `DecayPolicy::None`) |
| `FusionStrategy` ersetzt `fusion_weights` | additiv via `#[serde(alias)]` | Rebuild gespeicherter Query-Presets nicht nötig; Default wechselt von impliziter Gewichtssumme zu RRF — **Verhaltensänderung dokumentationspflichtig** |
| `memfuse-quant`, `VamanaIndex` | additiv (neuer Index-Typ) | opt-in über `CollectionConfig`; `HnswIndex` bleibt Default |
| `IoBackend` | additiv (`IoBackendSelector::Auto` default) | keine; `TokioBlockingBackend` = Ist-Zustand |
| `memfuse-kv` | additiv, eigenes Crate, Feature-Flag `context-cache` | keine für Konsumenten ohne Feature |
| `ContextCompactionStrategy`-Rename | **brechend** (Typname) | einmaliges `sed`-Rename in Downstream-Imports |
| `MetadataFilter: TryFrom<FilterExpr>` | additiv | keine — bestehender Builder-Aufrufpfad unverändert |
| `SandboxBridge` → `#[async_trait]` | **potenziell brechend** für `impl SandboxBridge`-Implementoren mit exotischen Lifetime-Bounds | i. d. R. keine Änderung nötig (async-trait-Makro toleriert Standardfälle) |
| `AuditLog<S>` generisch | additiv (Default bleibt `LsmStorage`) | nur betroffen, wer `AuditLog` explizit mit konkretem Typ annotiert hatte |

---

## 11. Priorisierte Risikoauflösung (Referenz auf Abschnitt 15.2 des Ist-Dokuments)

| Risiko # (Ist-Dok.) | Status in v2 |
|---|---|
| 1 — `MetadataFilter`/`FilterExpr`-Dopplung | Gelöst: expliziter `TryFrom`-Konvertierungspfad, §7.2 |
| 2 — `CompactionStrategy`-Namenskollision | Gelöst: Rename zu `ContextCompactionStrategy`, §7.3 |
| 3 — `SandboxBridge` RPITIT-Inkonsistenz | Gelöst: Vereinheitlichung auf `#[async_trait]`, §8.3 |
| 4 — `CrossEncoderReranker` Doppeldefinition | **Nicht in dieser Spezifikation adressiert** — erfordert Einzelanalyse der Feature-Flag-Kombinatorik in `memfuse-embed/src/reranker.rs`, siehe Ist-Dokument §15.3 |
| 5 — Fehlervariante geht an Außengrenze verloren | Gelöst: `MemFuseErrorCode`/`FfiError`, §2.1 + §9 |
| 6 — `AuditLog` hart an `LsmStorage` gebunden | Gelöst: `AuditLog<S: StorageEngine>`, §8.2 |
| 7 — Zwei `CheckpointGuard`/`StateCheckpoint`-Paare | **Nicht strukturell verändert** (Empfehlung Ist-Dok. war Doku-Klärung, kein API-Fix) — Modulkommentare gemäß ADR-011 ergänzen |
| 8 — `search_filtered`-Default-Error stiller Fallstrick | Empfehlung bleibt bestehen: Integrationstest pro `VectorIndex`-Implementor (jetzt inkl. `VamanaIndex`) ergänzen, der `Some(filter)` gegen den Default-Error-Pfad prüft |

---

## 12. Statistische Zusammenfassung v2 (Delta zum Ist-Dokument)

| Metrik | Ist-Dokument | v2 (Delta) |
|---|---|---|
| Anzahl Crates | 14 | **16** (+ `memfuse-quant`, `memfuse-kv`) |
| Neue Kern-Traits (dyn-safe, async) | — | + `IoBackend`, `ContextCacheBridge`, `Foldable`, `MemoryLifecycleManager` (4) |
| Neue Fundament-Typen (`memfuse-core`) | — | `MemFuseErrorCode`, `FfiError`, `MemoryGovernance`, `FusionStrategy`, `EmbeddingCodec`-Marker (5 Kernkonzepte) |
| Neue `VectorIndex`-Implementoren | 1 (`HnswIndex`) | **2** (+ `VamanaIndex`, disk-resident) |
| Gelöste Risiken aus Ist-Dokument §15.2 | 0/8 | **5/8** (Rest: Einzelanalyse bzw. reine Doku-Fixes) |
| Kompressionsfaktor Embedding (worst-case, Binary+Rescoring) | — | bis 32× (Speicher), bis 32× (Durchsatz), ~96 % Recall-Erhalt (Referenzwerte aus Sentence-Transformers-Benchmarks) |

---

## 13. Nicht adressierte, für Folgeanalyse empfohlene Bereiche

- **`CrossEncoderReranker`-Doppeldefinition** (Risiko #4, Ist-Dok.) — eigene Feature-Flag-Analyse nötig, bevor ein Rescoring-Reranker (§3) sauber eingehängt werden kann.
- **`memfuse-core/src/ipc/`-FlatBuffers-Schema** — für Zero-Copy-Python-Interop (`_fb`-Methoden, `memfuse-py`) sollte geprüft werden, ob `KvCacheRef` (§6) ebenfalls über FlatBuffers statt PyO3-Objektkonvertierung transportiert werden kann — konsistent mit dem bestehenden Zero-Copy-Anspruch.
- **Cluster-Feature** (`memfuse-cluster`, archiviert) — jede Multi-Node-Erweiterung von `memfuse-kv` (verteiltes KV-Cache-Tiering über Knoten, analog Harvest/Aqua, §0 R4) setzt eine Reaktivierung dieses Crates voraus; außerhalb des Scopes dieser Spezifikation.
- **Benchmark-Verifikation** aller in §0 zitierten Kompressions-/Latenzangaben gegen die tatsächliche MemFuse-Workload (Datensatzgröße, Embedding-Dimension, Zielhardware) vor Produktivübernahme — die zitierten Zahlen stammen aus externen Referenzbenchmarks, nicht aus MemFuse-eigenen Messungen.
