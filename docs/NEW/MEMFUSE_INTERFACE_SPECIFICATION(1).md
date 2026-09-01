# MemFuse — Mikrofein-Schnittstellenspezifikation
**Stand:** 2026-08-30 (HEAD: ba861c68)  
**Scope:** 15 Workspace-Crates · 5-Layer-DAG · Alle öffentlichen & crate-internen Schnittstellen  
**Zweck:** Vollständige Kontextquelle der gesamten Codebasis für Entwickler, Agenten und Architektur-Entscheidungen

---

## Inhaltsverzeichnis

1. [DAG-Übersicht & Abhängigkeitsmatrix](#1-dag-übersicht--abhängigkeitsmatrix)
2. [Layer 0 — `memfuse-core`](#2-layer-0--memfuse-core)
3. [Layer 1 — `memfuse-store`](#3-layer-1--memfuse-store)
4. [Layer 1 — `memfuse-index`](#4-layer-1--memfuse-index)
5. [Layer 1 — `memfuse-text`](#5-layer-1--memfuse-text)
6. [Layer 1 — `memfuse-graph`](#6-layer-1--memfuse-graph)
7. [Layer 1 — `memfuse-crypto`](#7-layer-1--memfuse-crypto)
8. [Layer 1 — `memfuse-checkpoint`](#8-layer-1--memfuse-checkpoint)
9. [Layer 2 — `memfuse-db`](#9-layer-2--memfuse-db)
10. [Layer 3 — `memfuse-ollama`](#10-layer-3--memfuse-ollama)
11. [Layer 3 — `memfuse-embed`](#11-layer-3--memfuse-embed)
12. [Layer 3 — `memfuse-agent`](#12-layer-3--memfuse-agent)
13. [Layer 3 — `memfuse-router`](#13-layer-3--memfuse-router)
14. [Layer 3 — `memfuse-py`](#14-layer-3--memfuse-py)
15. [Layer 4 — `memfuse-mcp`](#15-layer-4--memfuse-mcp)
16. [Layer 4 — `memfuse-tauri`](#16-layer-4--memfuse-tauri)
17. [Crate-übergreifende Datenflüsse](#17-crate-übergreifende-datenflüsse)
18. [Bekannte Schnittstellen-Besonderheiten & Fallstricke](#18-bekannte-schnittstellen-besonderheiten--fallstricke)

---

## 1. DAG-Übersicht & Abhängigkeitsmatrix

```
Layer 0:  memfuse-core
          └─ Basis für alle anderen Crates. Kein Upstream.

Layer 1:  memfuse-store      → memfuse-core, memfuse-crypto
          memfuse-index      → memfuse-core, memfuse-graph
          memfuse-text       → memfuse-core
          memfuse-graph      → memfuse-core
          memfuse-crypto     → memfuse-core
          memfuse-checkpoint → memfuse-core

Layer 2:  memfuse-db         → memfuse-core, memfuse-store, memfuse-index,
                               memfuse-text, memfuse-graph, memfuse-checkpoint,
                               memfuse-ollama (für Contextual Retrieval),
                               memfuse-embed (optional, Feature-gated)

Layer 3:  memfuse-ollama     → memfuse-core
          memfuse-embed      → memfuse-core  (ONNX optional)
          memfuse-agent      → memfuse-core, memfuse-db, memfuse-store,
                               memfuse-checkpoint, memfuse-graph
          memfuse-router     → memfuse-core, memfuse-db, memfuse-store,
                               memfuse-ollama, memfuse-mcp  ⚠ DAG-Verletzung
          memfuse-py         → memfuse-core, memfuse-db

Layer 4:  memfuse-mcp        → memfuse-core, memfuse-db, memfuse-crypto,
                               memfuse-ollama, memfuse-agent (optional)
          memfuse-tauri      → memfuse-core, memfuse-db, memfuse-graph,
                               memfuse-ollama
```

**Abhängigkeitsmatrix (Zeile=Consumer, Spalte=Provider):**

| Crate | core | store | index | text | graph | crypto | ckpt | db | ollama | embed | agent | mcp |
|---|:---:|:---:|:---:|:---:|:---:|:---:|:---:|:---:|:---:|:---:|:---:|:---:|
| store | ✓ | — | — | — | — | ✓ | — | — | — | — | — | — |
| index | ✓ | — | — | — | ✓ | — | — | — | — | — | — | — |
| text | ✓ | — | — | — | — | — | — | — | — | — | — | — |
| graph | ✓ | — | — | — | — | — | — | — | — | — | — | — |
| crypto | ✓ | — | — | — | — | — | — | — | — | — | — | — |
| checkpoint | ✓ | — | — | — | — | — | — | — | — | — | — | — |
| db | ✓ | ✓ | ✓ | ✓ | ✓ | — | ✓ | — | ✓ | opt | — | — |
| ollama | ✓ | — | — | — | — | — | — | — | — | — | — | — |
| embed | ✓ | — | — | — | — | — | — | — | — | — | — | — |
| agent | ✓ | ✓ | — | — | — | — | ✓ | ✓ | — | — | — | — |
| router | ✓ | ✓ | — | — | — | — | — | ✓ | ✓ | — | — | ⚠✓ |
| py | ✓ | — | — | — | — | — | — | ✓ | — | — | — | — |
| mcp | ✓ | — | — | — | — | ✓ | — | ✓ | ✓ | — | opt | — |
| tauri | ✓ | — | — | — | ✓ | — | — | ✓ | ✓ | — | — | — |

---

## 2. Layer 0 — `memfuse-core`

**Zweck:** Lingua Franca des Workspace. Definiert alle Kern-Typen, Traits, Error-Enum und Transaktions-Primitive. Keine I/O, kein async (ausser über Trait-Definitionen).  
**`unsafe`:** `#![deny(unsafe_code)]`  
**LOC:** ~7.361

### 2.1 Öffentliche Module & Re-Exporte

```
memfuse_core::
├── error          → MemFuseError, Result<T>
├── error_dto      → MemFuseErrorDto  (FFI-sicher, serialisierbar)
├── ipc            → FlatBuffers-generierte IPC-Typen (memfuse_generated)
├── seq_log        → SeqLogEntry, SequenceLog
├── snapshot       → SnapshotRegistry, SnapshotGuard
├── traits         → StorageEngine, VectorIndex, TextIndex, GraphIndex,
│                    TextEmbeddingEngine, CheckpointCoordinator, Checkpoint,
│                    MemoryLifecycleManager, DistanceCalculator
├── tx_buffer      → TxBuffer<T>, IndexOp<T>, TxBufferConfig
└── types          → (alle Domain-Typen, siehe 2.3)
```

### 2.2 Fehler-Hierarchie — `MemFuseError`

```rust
#[non_exhaustive]
pub enum MemFuseError {
    // Core & Logic
    Internal(String),
    InvalidInput(String),
    NotFound(String),
    PolicyViolation(String),
    // Storage Engine
    Storage(String),
    Io(#[from] std::io::Error),
    WalCorruption { offset: u64, reason: String },
    ChecksumMismatch { path: String, block_id: u64 },
    // Transactions
    Transaction(String),
    TransactionTimeout { tx_id: u64, elapsed_ms: u64 },
    Conflict(String),
    InvalidSequenceNumber(u64),
    // Index & Search
    Index(String),
    EmbeddingDimensionMismatch { expected: usize, got: usize },
    // Serialization
    Serialization(String),
    // FFI
    Encoding(String),
    // Capabilities
    CapabilityUnsupported { capability: String, reason: String },
}

// Konstruktor-Shortcuts (bevorzugt gegenüber ::new):
MemFuseError::invalid_input(msg)          // → InvalidInput
MemFuseError::not_found(msg)              // → NotFound
MemFuseError::capability_unsupported(cap, reason)  // → CapabilityUnsupported
```

**Invariante:** Keine neue Error-Enum in Downstream-Crates anlegen. Neue Varianten NUR unten anhängen (binäre Kompatibilität). Downstream-Crates brauchen Wildcard-Match-Arm (`_ => ...`).

### 2.3 Domain-Typen (`types::domain`)

```rust
// ID-Primitive (alle #[repr(transparent)] u64-Newtypes)
pub struct DocId(pub u64);
impl DocId {
    pub const MAX: Self;
    pub const MIN: Self;
    pub fn new(id: u64) -> Self;
    pub fn inner(self) -> u64;
    pub fn from_key(key: &str) -> Result<Self>;  // BLAKE3 8-Byte Trunkierung
}

pub struct EntityId(pub u64);
impl EntityId {
    pub fn new(id: u64) -> Self;
    pub fn inner(self) -> u64;
    pub fn as_bytes(&self) -> Vec<u8>;
    pub fn from_doc_id(doc_id: DocId) -> Self;
    pub fn from_key(key: &str) -> Result<Self>;  // fallible, bevorzugt
    // From<&str> / From<String>: infallibel (parses u64 oder BLAKE3)
}

pub struct TxId(pub u64);
impl TxId {
    pub const INVALID: Self;           // TxId(0) — Sentinel für nicht-initialisiert
    pub const INTERNAL_BASE: Self;     // u64::MAX - 1_000_000 — System-Transaktionen
    pub fn new(id: u64) -> Self;
    pub fn inner(self) -> u64;
    pub fn is_valid_origin(&self) -> bool;  // true wenn nicht INVALID und nicht SystemTime-basiert
}

// Konstanten
pub const TOMBSTONE_BIT: u64 = 1 << 63;      // Bit 63 — Lösch-Tombstone-Flag
pub const EXPIRY_METADATA_KEY: &str = "__expires_at_seq";
pub const MAX_SEARCH_K: usize = 1_000;        // Globale Such-Ergebnis-Obergrenze

// Graph-Entitäten
pub struct Entity { pub id: EntityId, pub name: String, pub entity_type: String }
impl Entity {
    pub fn new(id, name, entity_type) -> Self;
    pub fn try_new(id, name, entity_type) -> Result<Self>;   // validiert Längen
}

pub struct Edge {
    pub from: EntityId, pub to: EntityId, pub label: String, pub weight: f32,
    pub valid_from: Option<TxId>,   // bi-temporal: None = "seit jeher"
    pub valid_to:   Option<TxId>,   // bi-temporal: None = "weiterhin gültig"
}
impl Edge {
    pub fn new(from, to, label) -> Self;          // weight=1.0, validity=None
    pub fn try_new(from, to, label) -> Result<Self>;
    pub fn with_weight(mut self, weight: f32) -> Self;
    pub fn with_validity(mut self, from: Option<TxId>, to: Option<TxId>) -> Self;
}

// Workflow-State (Savepoint)
pub struct WorkflowState { pub tx: TxId, pub graph_hash: String }

// Vektoren & Ähnlichkeit
pub struct Embedding { data: Vec<f32> }
impl Embedding {
    pub fn new(data: Vec<f32>) -> Self;
    pub fn dim(&self) -> usize;
    pub fn as_slice(&self) -> &[f32];
    pub fn l2_norm(&self) -> f32;
    pub fn normalize(&self) -> Self;
}

pub struct ScoredDocument { pub doc_id: DocId, pub score: f32 }

pub enum DistanceMetric { Cosine, Euclidean, DotProduct }
impl DistanceMetric {
    pub fn compute(&self, a: &[f32], b: &[f32]) -> Result<f32>;
    pub fn compute_u8(&self, a: &[u8], b: &[u8]) -> Result<u32>;
}

// Kognitive Gedächtnistypen
pub enum MemoryType {
    Episodic,    // Ereignisse — TTL kurz, schneller Decay
    Semantic,    // Fakten — persistent, kein Decay
    Procedural,  // Workflows — persistent, moderater Decay
    Working,     // Kurzzeit — sehr kurze TTL
}
impl MemoryType {
    pub fn default_decay(&self) -> DecayFunction;
    pub fn default_ttl_tx(&self) -> Option<u64>;
    pub fn as_metadata_key(&self) -> &'static str;   // "__memory_type"
}

// PPR-Konfiguration
pub struct PprConfig {
    pub damping_factor: f32,       // default: 0.85
    pub convergence_epsilon: f32,  // default: 1e-6
    pub max_iterations: usize,     // default: 100
    pub top_k: usize,              // default: 20
}
```

### 2.4 SAOS-Typen (`types::saos`) — Hybrid-Query & Kontext

```rust
// Fusion-Gewichte — müssen exakt auf 1.0 summieren
pub struct FusionWeights { vector: f32, text: f32, graph: f32, metadata: f32 }
impl FusionWeights {
    pub fn new(vector, text, graph) -> Result<Self>;  // normalisiert, NaN/Inf/neg Guard
    pub fn vector() -> f32;
    pub fn text() -> f32;
    pub fn graph() -> f32;
    pub fn metadata() -> f32;  // immer 0.0 (reserviert)
}

// Graph-Traversal-Strategie
pub enum GraphTraversalStrategy {
    Hops { max_hops: usize },                    // Standard: 3-Hop BFS mit Decay
    PersonalizedPageRank(PprConfig),              // PPR Power-Iteration
}

// Unified 4-Signal Query
pub struct HybridQuery {
    pub text_query:         Option<String>,
    pub vector_query:       Option<Vec<f32>>,
    pub graph_start_node:   Option<String>,
    pub graph_strategy:     GraphTraversalStrategy,
    pub fusion_weights:     FusionWeights,
    pub filter:             Option<FilterExpr>,
    pub same_community_as:  Option<EntityId>,     // Community-Filter
    pub memory_type_filter: Option<Vec<MemoryType>>,
    pub k:                  usize,
}
impl HybridQuery {
    pub fn builder() -> HybridQueryBuilder;
}

// Builder-Pattern für HybridQuery
pub struct HybridQueryBuilder { ... }
impl HybridQueryBuilder {
    pub fn new() -> Self;
    pub fn with_text_query(self, q) -> Self;
    pub fn with_vector_query(self, v: Vec<f32>) -> Self;
    pub fn with_graph_start_node(self, start) -> Self;
    pub fn with_graph_strategy(self, strategy) -> Self;
    pub fn with_fusion_weights(self, weights) -> Self;
    pub fn with_filter(self, filter: FilterExpr) -> Self;
    pub fn with_same_community_as(self, entity_id: EntityId) -> Self;
    pub fn with_memory_type_filter(self, types: Vec<MemoryType>) -> Self;
    pub fn with_k(self, k: usize) -> Self;
    pub fn build(self) -> Result<HybridQuery>;   // validiert k <= MAX_SEARCH_K
}

// Kontext-Chunk (eine Retrieval-Einheit)
pub struct ContextChunk {
    pub doc_id:            DocId,
    pub content:           String,
    pub relevance:         f32,
    pub token_count:       usize,
    pub metadata:          Option<serde_json::Value>,
    pub contextual_prefix: Option<String>,        // Anthropic Contextual Retrieval
    pub links:             Vec<MemoryLink>,        // A-MEM Zettelkasten (ADR-038)
}
impl ContextChunk {
    pub fn combined_text_owned(&self) -> String;  // prefix + "\n\n" + content
    pub fn combined_token_count(&self) -> usize;
    pub fn has_context_prefix(&self) -> bool;
}
// TryFrom<SearchResult> ist implementiert (in memfuse-db)

// A-MEM Memory Link
pub struct MemoryLink { pub target: DocId, pub relation: LinkRelation, pub created_at_tx: TxId }
pub enum LinkRelation { Elaborates, Contradicts, Supersedes, References }

// Token-Budget
pub struct TokenBudget {
    pub max_tokens: usize,
    pub reserve_tokens: usize,
    pub strategy: BudgetStrategy,
}
impl TokenBudget {
    pub fn new(max_tokens, reserve_tokens) -> Self;
    pub fn for_model(model: &str) -> Self;    // "llama3", "gpt-4", etc.
    pub fn with_reserved(self, system, answer) -> Self;
    pub fn effective_limit(&self) -> usize;
    pub fn available(&self) -> usize;
    pub fn consume(&mut self, tokens: usize);
    pub fn consumed(&self) -> usize;
}

pub enum BudgetStrategy { TopK, CumulativeTokens, ScoreThreshold { min_score: f32 } }

// Kontext-Fenster (Ausgabe nach Budget-Trimming)
pub struct ContextWindow {
    pub chunks:        Vec<ContextChunk>,
    pub total_tokens:  usize,
    pub truncated:     bool,
}
```

### 2.5 Wichtigkeits-Typen (`types::importance`)

```rust
pub struct ImportanceScore(f32);   // [0.0, 1.0] geclampte f32
impl ImportanceScore {
    pub fn new(raw: f32) -> Self;  // clamp(0.0, 1.0)
    pub fn value(&self) -> f32;
}

pub enum DecayFunction {
    None,
    Exponential { half_life_tx: u64 },
    Linear { decay_per_tx: f32 },
}
impl DecayFunction {
    pub fn decay_factor(&self, created_at_tx: TxId, now_tx: TxId) -> f32;
}

pub struct MemoryImportance {
    pub base_score:     ImportanceScore,
    pub decay:          DecayFunction,
    pub created_at_tx:  TxId,
}
impl MemoryImportance {
    pub fn new(base_score, decay, created_at_tx) -> Self;
    pub fn effective_score(&self, now_tx: TxId) -> f32;   // base * decay_factor
}
```

### 2.6 Filter-Typen (`types::filter`)

```rust
pub enum FilterExpr {
    Eq   { field: String, value: serde_json::Value },
    Ne   { field: String, value: serde_json::Value },
    Gt   { field: String, value: serde_json::Value },
    Gte  { field: String, value: serde_json::Value },
    Lt   { field: String, value: serde_json::Value },
    Lte  { field: String, value: serde_json::Value },
    And  { filters: Vec<FilterExpr> },
    Or   { filters: Vec<FilterExpr> },
    Not  { filter: Box<FilterExpr> },
    In   { field: String, values: Vec<serde_json::Value> },
    Exists { field: String },
}
impl FilterExpr {
    pub fn evaluate(&self, metadata: &serde_json::Value) -> bool;
}
```

### 2.7 Kern-Traits

```rust
// StorageEngine — abstrakte LSM-Persistenz
#[async_trait]
pub trait StorageEngine: Send + Sync + 'static {
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;
    async fn get_at_seq(&self, key: &[u8], seq: u64) -> Result<Option<Vec<u8>>>;   // MVCC
    async fn put(&self, tx_id: TxId, key: &[u8], value: &[u8]) -> Result<()>;
    async fn put_batch(&self, tx_id: TxId, entries: &[(Vec<u8>, Vec<u8>)]) -> Result<()>;  // Default: sequenziell
    async fn delete(&self, tx_id: TxId, key: &[u8]) -> Result<()>;
    async fn delete_many(&self, tx_id: TxId, keys: Vec<Vec<u8>>) -> Result<u64>;   // Default: seq.
    async fn delete_prefix(&self, tx_id: TxId, prefix: &[u8]) -> Result<u64>;     // Default: scan + delete_many
    async fn commit(&self, tx_id: TxId) -> Result<()>;
    async fn rollback(&self, tx_id: TxId) -> Result<()>;      // nur uncommitted
    async fn rollback_to_tx(&self, tx_id: TxId) -> Result<()>; // physisch rückgängig machen
    async fn flush(&self) -> Result<()>;
    async fn stats(&self) -> Result<StorageStats>;
    async fn last_seq_no(&self) -> Result<u64>;
    async fn last_tx_id(&self) -> Result<TxId>;
    async fn pin_checkpoint(&self, seq_no: u64) -> Result<()>;
    async fn unpin_checkpoint(&self, seq_no: u64) -> Result<()>;
    async fn scan_prefix(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>>;
    async fn scan_prefix_at(&self, prefix: &[u8], seq_no: u64) -> Result<...>;   // Default: CapabilityUnsupported
    async fn scan(&self, start: Bound<&[u8]>, end: Bound<&[u8]>) -> Result<...>;
}

// VectorIndex — abstrakte HNSW-Vektorsuche
#[async_trait]
pub trait VectorIndex: Send + Sync + 'static {
    async fn insert(&self, tx: TxId, id: DocId, embedding: &[f32]) -> Result<()>;
    async fn insert_batch(&self, tx: TxId, vectors: &[(DocId, &[f32])]) -> Result<()>;
    async fn all_doc_ids(&self) -> Result<Vec<DocId>>;                 // Default: leer
    async fn search(&self, query: &[f32], k: usize) -> Result<Vec<ScoredDocument>>;
    async fn search_at(&self, query, k, seq_no) -> Result<...>;        // Default: CapabilityUnsupported
    async fn search_filtered(&self, query, k, filter: Option<&dyn Fn(DocId)->bool>) -> Result<...>;
    async fn delete(&self, tx: TxId, id: DocId) -> Result<()>;
    async fn commit(&self, tx: TxId) -> Result<()>;
    async fn rollback(&self, tx: TxId) -> Result<()>;
    async fn rollback_to_tx(&self, tx_id: TxId) -> Result<()>;
    async fn last_tx_id(&self) -> Result<u64>;
    async fn len(&self) -> usize;
    async fn is_empty(&self) -> bool;                                   // Default: len==0
    async fn stats(&self) -> Result<VectorIndexStats>;
    fn is_rebuild_required(&self) -> bool;                              // Default: false
    fn trigger_rebuild_async(&self);                                    // Default: no-op
}

// TextIndex — BM25/InvertedIndex
#[async_trait]
pub trait TextIndex: Send + Sync + 'static {
    async fn search(&self, query: &str, k: usize) -> Result<Vec<ScoredDocument>>;
    async fn search_at(&self, query, k, seq_no) -> Result<...>;        // Default: CapabilityUnsupported
    async fn insert(&self, tx: TxId, id: DocId, text: &str) -> Result<()>;
    async fn delete(&self, tx: TxId, id: DocId) -> Result<()>;
    async fn commit(&self, tx: TxId) -> Result<()>;
    async fn rollback(&self, tx: TxId) -> Result<()>;
    async fn rollback_to_tx(&self, tx_id: TxId) -> Result<()>;
    async fn last_tx_id(&self) -> Result<u64>;
    async fn len(&self) -> usize;
    async fn stats(&self) -> Result<TextIndexStats>;
}

// GraphIndex — CSR-basierter Entity-Graph
#[async_trait]
pub trait GraphIndex: Send + Sync + 'static {
    // Lesen
    async fn traverse(&self, start: EntityId, max_hops: usize) -> Result<Vec<(EntityId, f32)>>;
    async fn neighbors(&self, start: EntityId) -> Result<Vec<EntityId>>;   // Default: 1-Hop traverse
    async fn multi_traverse(&self, starts: &[EntityId], max_hops) -> Result<...>;  // Default: kombiniert
    async fn traverse_at(&self, start, hops, seq_no) -> Result<...>;              // Default: CapabilityUnsupported
    async fn traverse_at_time(&self, start, hops, as_of: TxId) -> Result<...>;   // Default: CapabilityUnsupported
    async fn personalized_page_rank(&self, seeds, config) -> Result<...>;         // Default: CapabilityUnsupported
    // Schreiben
    async fn add_entity(&self, tx: TxId, entity: Entity) -> Result<()>;
    async fn add_edge(&self, tx: TxId, edge: Edge) -> Result<()>;
    async fn add_bidirectional(&self, tx, from, to, label) -> Result<()>;         // Default: 2x add_edge
    async fn remove_edge(&self, tx, from, to) -> Result<()>;                      // Default: no-op
    // Transaktion
    async fn commit(&self, tx: TxId) -> Result<()>;
    async fn rollback(&self, tx: TxId) -> Result<()>;
    async fn rollback_to_tx(&self, tx_id: TxId) -> Result<()>;
    async fn last_tx_id(&self) -> Result<u64>;
    async fn len(&self) -> usize;
    async fn stats(&self) -> Result<GraphIndexStats>;
}
// ⚠ TxId-Origin-Invariant: tx MUSS aus Collection::allocate_tx() oder TxId::INTERNAL_BASE-Bereich stammen

// TextEmbeddingEngine
#[async_trait]
pub trait TextEmbeddingEngine: Send + Sync + 'static {
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;  // Default: seq. Calls
}

// MemoryLifecycleManager
#[async_trait]
pub trait MemoryLifecycleManager: Send + Sync {
    async fn sweep(&self, now_tx: TxId) -> Result<LifecycleSweepReport>;
    async fn plan_consolidation(&self, candidates: &[DocId]) -> Result<Vec<ConsolidationAction>>;
}

pub enum ConsolidationAction { Keep { doc_id }, Merge { source_ids, summary_hint }, Supersede { old_id, new_id }, Drop { doc_id } }
pub struct LifecycleSweepReport { swept_count: u64, deleted_by_ttl, deleted_by_decay, skipped_pinned }
```

### 2.8 Transaktions-Primitive

```rust
// TxBuffer<T: Clone> — Sharded Staging für Index-Operationen
pub struct TxBuffer<T: Clone> { ... }
impl<T: Clone + Send + Sync + 'static> TxBuffer<T> {
    pub fn new() -> Self;
    pub fn new_with_config(shard_count: usize, tx_timeout: Duration) -> Self;
    pub fn has_tx(&self, tx: TxId) -> bool;
    pub fn begin(&self, tx: TxId);
    pub fn stage(&self, tx: TxId, op: IndexOp<T>) -> Result<()>;
    pub fn stage_bounded(&self, tx: TxId, op: IndexOp<T>) -> Result<()>;  // prüft MAX_OPS_PER_TX
    pub fn stage_many(&self, tx: TxId, ops: impl IntoIterator<Item = IndexOp<T>>) -> Result<()>;
    pub fn validate_pending_ops(&self, tx: TxId) -> Result<()>;
    pub fn drain(&self, tx: TxId) -> Vec<IndexOp<T>>;
    pub fn discard(&self, tx: TxId);
    pub fn len(&self) -> usize;
    pub fn reap_orphans(&self) -> Vec<TxId>;   // entfernt Transaktionen > timeout
}

pub enum IndexOp<T: Clone> {
    Insert { tx: TxId, doc_id: DocId, data: T },
    Delete { tx: TxId, doc_id: DocId },
}

// SnapshotRegistry — MVCC Read-Isolation
pub struct SnapshotRegistry { ... }
impl SnapshotRegistry {
    pub fn new() -> Self;
    pub fn register(self: &Arc<Self>, seq_no: u64) -> SnapshotGuard;  // RAII Pin
    pub fn min_active_seqno(&self) -> u64;
    pub fn pin(&self, seq_no: u64);
    pub fn unpin(&self, seq_no: u64);
}

pub struct SnapshotGuard { ... }   // Drop → unpin automatisch
impl SnapshotGuard {
    pub fn seq_no(&self) -> u64;
}

// SequenceLog — Monoton steigende SeqNo-Verfolgung
pub struct SequenceLog { ... }
pub struct SeqLogEntry { pub tx_id: TxId, pub seq_no: u64 }
```

---

## 3. Layer 1 — `memfuse-store`

**Zweck:** LSM-Tree Storage Engine mit WAL, MemTable, SSTables, MVCC.  
**Implementiert:** `StorageEngine` für `LsmStorage`  
**`unsafe`:** `#![forbid(unsafe_code)]`  
**LOC:** ~10.658

### 3.1 Öffentliche API

```rust
// Primärer Einstiegspunkt
pub struct LsmStorage { ... }

pub struct LsmConfig {
    pub path:               PathBuf,
    pub max_memtable_size:  usize,      // Default: 64 MiB
    pub max_sst_size:       usize,
    pub sync_writes:        bool,
    pub tx_timeout:         Duration,   // Default: 30s
    pub compaction:         CompactionConfig,
    pub encryption_key:     Option<Vec<u8>>,
}

impl LsmStorage {
    pub async fn new(config: LsmConfig) -> Result<Self>;
    pub async fn force_flush(&self) -> Result<()>;
    pub async fn maybe_compact(&self) -> Result<bool>;
    pub fn shutdown(&self);
    pub async fn wait_shutdown(&self);
    pub async fn close(&self) -> Result<()>;
    pub fn spawn_tracked<F: Future + Send + 'static>(&self, future: F);
    pub async fn rollback_to_tx(&self, target_tx: TxId) -> Result<()>;
    // + vollständige StorageEngine-Trait-Implementierung (get, put, delete, commit, rollback, scan, ...)
}
```

### 3.2 WAL-Typen

```rust
pub struct Wal { ... }    // Write-Ahead-Log, HMAC V3 Format

pub struct WalEntry {
    pub seq_no: u64,
    pub tx_id: TxId,
    pub key:   Vec<u8>,
    pub value: Vec<u8>,   // leer = Tombstone
}
impl WalEntry {
    pub fn try_new(seq_no, tx_id, key, value, next_hmac) -> Result<Self>;
    pub fn compute_checksum_v3(seq_no, tx_id, op_type, key, value, prev_hmac, key_bytes) -> [u8; 32];
    pub fn tx_id(&self) -> TxId;
}
// WAL-Format: MFW3-Header (ADR-029). V1/V2 Abwärtskompatibilität + automatische Migration zu V3.
```

### 3.3 Compaction

```rust
pub struct CompactionConfig {
    pub max_sst_files:    usize,
    pub size_ratio:       f32,
    pub min_merge_files:  usize,
}

pub struct CompactionEngine { ... }
impl CompactionEngine {
    pub fn new(path, config, sstables, memtables, next_seq_no, snapshot_registry) -> Self;
    // Intern: Write-Temp-Then-Rename Pattern (ADR-042) für atomare SSTable-Erzeugung
}
```

### 3.4 Namespace-Konventionen

```
LSM-Schlüssel-Schema (Beispiel für Collection "default"):
__col:default:\x00  → Typ 0: Dokument-Embedding + vollständige Metadaten
__col:default:\x01  → Typ 1: Dokument-Metadaten (kein Embedding) für Hydration
__col:default:\x02  → Typ 2: Relation/Graph-Edges
__col:default:\x03  → Typ 3: Importance-Metadaten
__graph:entity:     → CSR-Graph Entitäten
__graph:edge:       → CSR-Graph Kanten
__graph:community:  → Community-Assignments
audit:{task_id}:    → Agent-Audit-Log-Einträge
ckpt:{name}:        → Checkpoint-Metadaten
```

---

## 4. Layer 1 — `memfuse-index`

**Zweck:** HNSW Vektorindex mit SIMD-Distanzberechnung, optionalem DiskANN.  
**Implementiert:** `VectorIndex` für `HnswIndex`  
**`unsafe`:** `#![deny(unsafe_code)]` (SIMD-Intrinsics in `distance.rs` erlaubt)  
**LOC:** ~7.374

### 4.1 Öffentliche API

```rust
// HNSW-Konfiguration (Builder-Pattern)
pub struct HnswConfig { dimension, max_elements, m, ef_construction, ef_search,
                        distance_metric, quantize, quantizer_recalibration_sample_size }
impl HnswConfig {
    pub fn new(dimension: usize) -> Self;
    pub fn max_elements(self, max) -> Self;
    pub fn m(self, m: usize) -> Self;                  // Default: 16, graph connectivity
    pub fn ef_construction(self, ef) -> Self;           // Default: 200
    pub fn ef_search(self, ef) -> Self;                 // Default: 50
    pub fn distance_metric(self, metric: DistanceMetric) -> Self;
    pub fn quantize(self, quantize: bool) -> Self;
    pub fn build(self) -> Result<HnswConfig>;           // validiert alle Werte
}

// HNSW Index
pub struct HnswIndex { ... }
impl HnswIndex {
    pub fn try_new(config: HnswConfig) -> Result<Self>;   // BEVORZUGT
    #[deprecated] pub fn new(config: HnswConfig) -> Self; // lazy validation
    pub fn quantizer(&self) -> &RwLock<Option<ScalarQuantizer>>;
    pub fn connectivity_score(&self) -> f64;
    pub fn check_connectivity(&self) -> Result<()>;
    pub fn is_rebuild_required(&self) -> bool;            // true wenn >20% gelöscht
    pub async fn rebuild(&self) -> Result<()>;
    pub fn compact_seq_log(&self, min_active_seqno: u64);
    pub fn all_doc_ids_from_map(&self) -> Vec<DocId>;     // für SSTable-Sync
    pub fn trigger_rebuild_async(&self) -> Option<JoinHandle<Result<()>>>;
    // + vollständige VectorIndex-Trait-Implementierung
}

// Rebuild-Status
pub enum RebuildStatus { NotRequired, InProgress, Completed, Failed(String) }
```

### 4.2 Distanzfunktionen (`distance.rs`)

```rust
// Öffentliche Low-Level Distanzfunktionen (SIMD-optimiert)
// ⚠ PANIC-Kontrakt (ADR-034): Längengleichheit wird via assert_eq! erzwungen (Release-aktiv)
pub fn cosine_distance(a: &[f32], b: &[f32]) -> f32;
pub fn euclidean_distance(a: &[f32], b: &[f32]) -> f32;
pub fn dot_product_distance(a: &[f32], b: &[f32]) -> f32;

// Runtime SIMD-Dispatch (automatisch):
// Priorität: AVX-512 > AVX2 > NEON (ARM) > Skalar
```

### 4.3 Quantisierung (`quantize.rs`)

```rust
pub struct ScalarQuantizer { ... }   // SQ8 — 8-Bit Skalare Quantisierung
// Intern: Matryoshka-kompatible Trunkierung (Grundlage für spätere MRL-Impl)
```

### 4.4 DiskANN (experimentell, `#[cfg(feature = "experimental-diskann")]`)

```rust
pub struct DiskAnnConfig { ... }
pub struct DiskAnnIndex { ... }
// VectorIndex implementiert, aber insert/delete → Err (noch nicht vollständig)
// Nutzt Write-Temp-Then-Rename + mmap für Datei-Operationen
// ⚠ Nicht in Collection integriert (ADR-013, ADR-037 Proposed)
```

### 4.5 HNSW-Persistenz (`persistence.rs`)

```rust
pub struct HnswHeader { ... }
pub struct MmapIndex { ... }   // Mmap-basierter In-Place-Lese-Zugriff für persistierte Indizes
```

---

## 5. Layer 1 — `memfuse-text`

**Zweck:** BM25-Volltextsuche mit invertiertem Index und deutscher Morphologie.  
**Implementiert:** `TextIndex` für `Bm25Scorer<S>`, `BM25MorphIndex`  
**`unsafe`:** `#![forbid(unsafe_code)]`  
**LOC:** ~3.833

### 5.1 Öffentliche API

```rust
// Haupt-Scorer (implementiert TextIndex)
pub struct Bm25Scorer<S: StorageEngine> { index: InvertedIndex<S> }
impl<S: StorageEngine> Bm25Scorer<S> {
    pub fn new(storage: Arc<S>, namespace: &str) -> Self;
    // TextIndex::search, insert, delete, commit, rollback, rollback_to_tx, last_tx_id, len, stats
}

// Morphologie-erweiterter Index
pub struct BM25MorphIndex { ... }   // InvertedIndex + GermanMorphTokenizer

// Invertierter Index (Kern-Implementierung)
pub struct InvertedIndex<S: StorageEngine> { ... }
impl<S: StorageEngine> InvertedIndex<S> {
    pub fn new(storage: Arc<S>, namespace: &str) -> Self;
    pub async fn search(&self, query: &str, k: usize) -> Result<Vec<ScoredDocument>>;
    pub async fn search_at(&self, query, k, seq_no) -> Result<...>;  // MVCC-isoliert
    pub async fn insert(&self, tx: TxId, id: DocId, text: &str) -> Result<()>;
    pub async fn delete(&self, tx: TxId, id: DocId) -> Result<()>;
    // + commit, rollback, rollback_to_tx, last_tx_id, len
}
```

### 5.2 Morphologie-Komponenten

```rust
// Deutsches Morphologie-Toolkit
pub struct GermanCompoundSplitter { ... }
impl GermanCompoundSplitter {
    pub fn split(&self, word: &str) -> Vec<String>;  // "Urlaubsantragsprozess" → ["Urlaub", "Antrag", "Prozess"]
}

pub struct MorphologicalTokenizer { ... }
pub fn normalize_umlauts(text: &str) -> String;    // ä→ae, ö→oe, ü→ue

// Tokenizer-Trait
pub trait Tokenizer: Send + Sync {
    fn tokenize(&self, text: &str) -> Vec<String>;
}
pub struct DefaultTokenizer;
pub struct GermanMorphTokenizer;

pub struct BM25 { ... }   // BM25-Scoring-Algorithmus (k1=1.2, b=0.75)
pub enum Language { English, German, Auto }
```

---

## 6. Layer 1 — `memfuse-graph`

**Zweck:** CSR-Graph für Entity-Relation-Traversal (Signal 3) + Session-DAG.  
**Implementiert:** `GraphIndex` für `CsrGraph`  
**`unsafe`:** `#![forbid(unsafe_code)]`  
**LOC:** ~5.092

### 6.1 CSR-Graph (`csr.rs`)

```rust
pub struct CsrGraphConfig {
    pub max_pending_before_compact: usize,  // Default: 1000
    pub compact_on_commit:          bool,
}

pub struct CsrGraph { inner: RwLock<GraphInner>, storage: Option<Arc<dyn StorageEngine>> }
impl CsrGraph {
    pub fn new() -> Self;
    pub fn with_config(config: CsrGraphConfig) -> Self;
    pub fn with_storage(storage: Arc<dyn StorageEngine>) -> Self;
    pub fn with_config_and_storage(config, storage) -> Self;
    pub fn set_storage(&mut self, storage: Arc<dyn StorageEngine>);

    // Direkte Einfüge-Methoden (ohne TxId, für interne Nutzung)
    pub fn insert_entity_direct(&self, entity: Entity) -> Result<()>;
    pub fn insert_edge_direct(&self, from: EntityId, to: EntityId, weight: f32) -> Result<()>;
    pub fn insert_edge_direct_with_validity(&self, from, to, weight, valid_from, valid_to) -> Result<()>;

    // Persistenz
    pub async fn persist_entity<S: StorageEngine + ?Sized>(storage: &S, entity: &Entity) -> Result<()>;
    pub async fn persist_edge<S: StorageEngine + ?Sized>(storage: &S, from, to, weight, ...) -> Result<()>;
    pub async fn delete_edge_persistence<S: StorageEngine + ?Sized>(storage: &S, from, to) -> Result<()>;
    pub async fn load_from_storage<S: StorageEngine + ?Sized>(storage: &S) -> Result<Self>;

    // Graph-Algorithmen
    pub fn compact(&self);                                      // CSR-Rebuild (vollständig)
    pub async fn compact_async(self: &Arc<Self>) -> Result<()>;
    pub async fn neighbors(&self, start: EntityId) -> Result<Vec<EntityId>>;
    pub fn pagerank(&self, damping: f32, iterations: usize) -> Vec<(EntityId, f32)>;

    // Statistiken
    pub fn entity_count(&self) -> usize;
    pub fn entity_exists(&self, id: EntityId) -> bool;
    pub fn edge_count(&self) -> usize;

    // + vollständige GraphIndex-Trait-Implementierung (traverse, traverse_at_time, ppr, add_entity, add_edge, commit, ...)
}

// Persisted Edge Payload (LSM-Serialisierung)
pub struct PersistedEdgePayload {
    pub weight: f32, pub label: String,
    pub valid_from: Option<u64>, pub valid_to: Option<u64>,
}
```

### 6.2 Personalized PageRank (`ppr.rs`)

```rust
// Intern (pub(crate)) — aufgerufen via GraphIndex::personalized_page_rank
pub(crate) fn compute_ppr(inner: &GraphInner, seed_nodes: &[EntityId], config: &PprConfig) -> Vec<(EntityId, f32)>;
// Power-Iteration mit L1-Norm Konvergenz-Check
// Sink-Node-Handling: Restart-Masse wird gleichmäßig auf Seed-Nodes verteilt
// Zero-Hang: Harte Obergrenze via config.max_iterations
// Determinismus: Tie-Breaking via EntityId-Numerik (bitidentische Ergebnisse)
```

### 6.3 Community Detection (`community.rs`)

```rust
pub struct CommunityDetectionConfig {
    pub max_iterations: usize,   // Default: 50
    pub seed: u64,               // RNG-Seed für deterministische Ausführung
}
pub struct CommunityAssignment { pub entity_id: EntityId, pub community_id: u64 }

pub async fn detect_communities(
    graph: &CsrGraph,
    config: CommunityDetectionConfig,
) -> Result<Vec<CommunityAssignment>>;
// Algorithmus: Label-Propagation (LPA) — deterministisch via fixiertem Seed + EntityId Tie-Breaking
// Ergebnis wird unter __graph:community:<entity_id> im LSM gespeichert
```

### 6.4 Session DAG (`session_dag.rs`)

```rust
pub type NodeIdx = usize;

pub struct AgentStateNode {
    pub idx: NodeIdx, pub prompt: String, pub response: String,
    pub parent: Option<NodeIdx>, pub created_at: u64,
}
pub struct DagEdge { pub from: NodeIdx, pub to: NodeIdx }

pub struct SessionBranchTree { nodes: RwLock<...>, edges: RwLock<...>, active_head: RwLock<NodeIdx> }
impl SessionBranchTree {
    pub fn new(root_prompt: String, root_response: String) -> Self;
    pub fn append_step(&self, prompt, response) -> Result<NodeIdx>;     // an active_head anhängen
    pub fn branch_from(&self, node_idx, prompt, response) -> Result<NodeIdx>;
    pub fn set_active_head(&self, node_idx: NodeIdx) -> Result<()>;
    pub fn path_to_head(&self) -> Vec<AgentStateNode>;
    pub fn children_of(&self, node_idx: NodeIdx) -> Vec<NodeIdx>;
    pub fn active_head(&self) -> NodeIdx;
    pub fn node_count(&self) -> usize;
    pub fn get_node(&self, node_idx: NodeIdx) -> Option<AgentStateNode>;
    pub async fn save<S: StorageEngine + ?Sized>(storage: &S, namespace: &str) -> Result<()>;
    pub async fn load<S: StorageEngine + ?Sized>(storage: &S, namespace: &str) -> Result<Self>;
}
// Lock-Hierarchie: nodes MUSS vor edges oder active_head akquiriert werden
```

---

## 7. Layer 1 — `memfuse-crypto`

**Zweck:** AES-256-GCM-SIV Verschlüsselung, HKDF Key-Derivation, WAL-HMAC-Integrität.  
**`unsafe`:** `#![cfg_attr(not(test), forbid(unsafe_code))]` (test-only unsafe für Zeroize-Test)  
**LOC:** ~1.144

### 7.1 Öffentliche API

```rust
// Primärer Export
pub use crypto::KeyManager as CryptoKey;

// KeyManager — AES-256-GCM-SIV mit pro-Datei HKDF-Ableitung
pub struct KeyManager { /* key: Zeroizing<[u8; 32]>, ... */ }
impl KeyManager {
    pub fn try_new(passphrase: &str, salt: &[u8]) -> Result<Self>;
    pub fn try_new_random_salt(passphrase: &str) -> Result<(Self, [u8; 32])>;
    pub fn derive_file_key(&self, file_id: &[u8]) -> Result<Self>;    // HKDF pro Datei
    pub fn integrity_key(&self) -> Result<[u8; 32]>;
    pub fn encrypt_auto_nonce(&self, data: &[u8]) -> Result<(Vec<u8>, [u8; 12])>;
    pub fn decrypt_auto_nonce(&self, ciphertext: &[u8], nonce_bytes: &[u8; 12]) -> Result<Vec<u8>>;
    pub fn emergency_wipe(&mut self);   // Zeroize Key-Material
    #[cfg(test)] pub fn inspect_key_bytes_for_test(&self) -> &[u8; 32];
}
// Nonce-Schema: 4-Byte OsRng Prefix + 4-Byte AtomicU64 Counter + 4-Byte File-HKDF
// → verhindert Nonce-Reuse bei parallelen Threads
```

### 7.2 Anti-Tamper (`anti_tamper.rs`)

```rust
// HMAC-Kette für WAL-Integritätsprüfung
// Verhindert tx_id-Manipulation ohne HMAC-Invalidierung (ADR-029)
```

### 7.3 WAL-Crypto (`wal_crypto.rs`)

```rust
pub struct EncryptedWal { ... }
impl EncryptedWal {
    pub fn new(km: KeyManager, path: &[u8]) -> Result<Self>;
    // Internes Format: HMAC-V3-Chain über alle WAL-Einträge
}
```

---

## 8. Layer 1 — `memfuse-checkpoint`

**Zweck:** Benannte, persistente Checkpoints und RAII-Rollback-Guard (ADR-011).  
**Implementiert:** `CheckpointCoordinator` für `PersistentCheckpointStore<S>`  
**`unsafe`:** `#![forbid(unsafe_code)]`  
**LOC:** ~1.166

### 8.1 Öffentliche API

```rust
pub const MAX_COMPONENTS: usize = 1000;
pub const MAX_CHECKPOINTS: usize = 10_000;

// Checkpoint-Metadaten
pub struct CheckpointMeta {
    pub name: String, pub collection_id: String,
    pub seq_no: u64,  pub tx_id: TxId,
    pub created_at_unix_secs: u64,
    pub metadata: serde_json::Value,
}

// StateCheckpoint — ein konkreter Savepoint
pub struct StateCheckpoint { ... }
impl StateCheckpoint {
    pub fn new(meta: CheckpointMeta, components: Vec<String>) -> Result<Self>;
    pub fn verify(&self) -> Result<()>;
    pub fn into_workflow_state(&self) -> WorkflowState;
}

// RAII-Guard — automatisches Rollback bei Drop wenn nicht committed
pub struct CheckpointGuard<S: StorageEngine> { ... }
impl<S: StorageEngine> CheckpointGuard<S> {
    pub async fn for_agent_step(storage: Arc<S>, tx: TxId) -> Result<Self>;
    pub fn checkpoint(&self) -> Result<&StateCheckpoint>;
    pub fn commit(mut self) -> Result<StateCheckpoint>;  // konsumiert Guard → kein Rollback
}
// Drop-Handler: rollback_to_tx() wenn nicht committed

// Persistenter Checkpoint-Store (CheckpointCoordinator-Impl)
pub struct PersistentCheckpointStore<S: StorageEngine> { ... }
impl<S: StorageEngine> PersistentCheckpointStore<S> {
    pub fn new(storage: Arc<S>, namespace: impl Into<String>) -> Self;
    pub fn create_guard(&self, tx_id: TxId) -> Result<CheckpointGuard<S>>;
    pub async fn create_checkpoint(&self, name, collection_id, seq_no, tx_id, meta) -> Result<CheckpointMeta>;
    pub async fn drop_checkpoint(&self, name: &str) -> Result<()>;
    pub async fn list_checkpoints(&self) -> Result<Vec<CheckpointMeta>>;
    pub async fn get_checkpoint(&self, name: &str) -> Result<Option<CheckpointMeta>>;
    pub async fn restore_checkpoint(&self, name: &str) -> Result<CheckpointMeta>;
    // Implementiert CheckpointCoordinator<Meta = CheckpointMeta>
}
```

---

## 9. Layer 2 — `memfuse-db`

**Zweck:** Orchestrator-Facade. Bindet alle Layer-1-Indizes zusammen, stellt die Collection-API und die MemFuse-Top-Level-API bereit.  
**`unsafe`:** `#![forbid(unsafe_code)]`  
**LOC:** ~12.753

### 9.1 Top-Level Facade — `MemFuse`

```rust
pub struct MemFuse { storage, collections: RwLock<HashMap<String, Arc<Collection>>>, ... }

pub struct MemFuseConfig {
    pub hnsw:              HnswConfig,
    pub compaction:        CompactionConfig,
    pub embedder:          Option<Arc<dyn TextEmbeddingEngine>>,
    pub repair_on_open:    bool,
    pub default_collection: String,
}

impl MemFuse {
    // Öffnen / Erstellen
    pub async fn open(path: impl AsRef<Path>) -> Result<Self>;
    pub async fn open_with_config(path, config: MemFuseConfig) -> Result<Self>;

    // Collection-Management
    pub async fn collection(&self, name: &str) -> Result<Arc<Collection<LsmStorage>>>;
    pub async fn list_collections(&self) -> Result<Vec<String>>;
    pub async fn drop_collection(&self, name: &str) -> Result<()>;
    pub fn allocate_tx(&self) -> Result<TxId>;

    // Convenience-Operationen auf Default-Collection
    pub async fn insert(&self, id, embedding, metadata) -> Result<()>;
    pub async fn insert_typed(&self, id, embedding, metadata, memory_type: MemoryType) -> Result<()>;
    pub async fn upsert(&self, id, embedding, metadata) -> Result<()>;
    pub async fn insert_many(&self, docs: &[(String, Vec<f32>, Option<Value>)]) -> Result<()>;
    pub async fn upsert_many(&self, docs: &[(String, Vec<f32>, Option<Value>)]) -> Result<()>;
    pub async fn get(&self, id: &str) -> Result<Option<Document>>;
    pub async fn get_at_snapshot(&self, id, seq_no: u64) -> Result<Option<Document>>;  // MVCC
    pub async fn last_committed_seq(&self) -> Result<u64>;
    pub async fn create_snapshot(&self) -> Result<u64>;
    pub async fn update(&self, id, embedding, metadata) -> Result<()>;
    pub async fn delete(&self, id: &str) -> Result<()>;
    pub async fn relate(&self, from, to, label) -> Result<()>;

    // Suche auf Default-Collection
    pub async fn search(&self, query: &[f32], k) -> Result<Vec<SearchResult>>;
    pub async fn search_with_filter(&self, query, k, filter: MetadataFilter) -> Result<...>;
    pub async fn search_with_filter_expr(&self, query, k, filter: FilterExpr) -> Result<...>;
    pub async fn search_text(&self, text, k) -> Result<Vec<SearchResult>>;
    pub async fn search_filtered(&self, text, query, k, filter) -> Result<...>;
    pub async fn hybrid_search(&self, text, query, k) -> Result<Vec<SearchResult>>;
    pub async fn hybrid_search_reranked(&self, text, query, k) -> Result<Vec<SearchResult>>;   // + Cross-Encoder
    pub async fn hybrid_search_with_weights(&self, text, query, k, weights: FusionWeights) -> Result<...>;
    pub async fn hybrid_search_with_strategy(&self, text, query, k, weights, filter, strategy, community) -> Result<...>;
    pub async fn hybrid_search_with_query(&self, query: HybridQuery) -> Result<Vec<SearchResult>>;
    pub async fn insert_text_only(&self, id, text, metadata) -> Result<()>;
    pub async fn upsert_text_only(&self, id, text, metadata) -> Result<()>;
    pub async fn search_filtered_at(&self, text, query, k, filter, seq_no) -> Result<...>;     // MVCC
}

pub struct SearchResult {
    pub id:       String,
    pub score:    f32,
    pub metadata: Option<serde_json::Value>,
}

pub struct Document {
    pub id:        String,
    pub embedding: Vec<f32>,
    pub metadata:  Option<serde_json::Value>,
}

pub struct DbStats {
    pub num_documents: usize,
    pub storage:       StorageStats,
    pub vector_index:  VectorIndexStats,
}
```

### 9.2 Collection API — `Collection<S, V>`

```rust
pub struct Collection<S: StorageEngine = LsmStorage, V: VectorIndex = HnswIndex> {
    storage: Arc<S>, vector_index: Arc<V>, text_index: Arc<InvertedIndex<S>>,
    graph_index: Arc<CsrGraph>, insert_lock: Mutex<()>,
    embedder: RwLock<Option<Arc<dyn TextEmbeddingEngine>>>,
    next_tx: Arc<AtomicU64>, name: String,
    reaper: Option<JoinHandle<()>>, orphan_reaper: Option<JoinHandle<()>>,
}

// CRUD (crud.rs)
impl<S, V> Collection<S, V> {
    pub async fn insert(&self, id, embedding, metadata) -> Result<()>;
    pub async fn insert_op(&self, id, embedding, metadata) -> Result<()>;       // intern (kein embed-Aufruf)
    pub async fn insert_text_only(&self, id, text, metadata) -> Result<()>;     // auto-embed
    pub async fn upsert_text_only(&self, id, text, metadata) -> Result<()>;
    pub async fn insert_typed(&self, id, embedding, metadata, memory_type) -> Result<()>;
    pub async fn insert_with_ttl(&self, id, embedding, metadata, ttl_tx: u64) -> Result<()>;
    pub async fn insert_many(&self, docs: &[(String, Vec<f32>, Option<Value>)]) -> Result<()>;
    pub async fn upsert(&self, id, embedding, metadata) -> Result<()>;
    pub async fn upsert_many(&self, docs: &[...]) -> Result<()>;
    pub async fn update(&self, id, embedding, metadata) -> Result<()>;
    pub async fn update_op(&self, id, embedding, metadata) -> Result<()>;
    pub async fn get(&self, id: &str) -> Result<Option<Document>>;
    pub async fn get_at_snapshot(&self, id, seq_no) -> Result<Option<Document>>;
    pub async fn delete(&self, id: &str) -> Result<()>;
    pub async fn delete_op(&self, id: &str) -> Result<()>;
    pub async fn scan_prefix(&self, prefix: &str) -> Result<Vec<(String, serde_json::Value)>>;
    pub async fn scan(&self, start: Option<&str>, end: Option<&str>) -> Result<Vec<(String, ...)>>;
}

// Suche (search.rs)
impl<S, V> Collection<S, V> {
    pub async fn search(&self, query: &[f32], k) -> Result<Vec<SearchResult>>;
    pub async fn search_with_filter(&self, query, k, filter: MetadataFilter) -> Result<...>;
    pub async fn search_with_filter_expr(&self, query, k, filter: FilterExpr) -> Result<...>;
    pub async fn search_text(&self, query, k) -> Result<Vec<SearchResult>>;
    pub async fn search_filtered(&self, text, query, k, filter) -> Result<...>;
    pub async fn search_filtered_at(&self, text, query, k, filter, seq_no) -> Result<...>;
    pub async fn hybrid_search(&self, text, query, k) -> Result<Vec<SearchResult>>;
    pub async fn hybrid_search_reranked(&self, text, query, k) -> Result<...>;    // Reranking via CrossEncoder
    pub async fn hybrid_search_with_weights(&self, text, query, k, weights) -> Result<...>;
    pub async fn hybrid_search_with_strategy(&self, text, query, k, weights, filter, strategy, community) -> Result<...>;
    pub async fn hybrid_search_with_query(&self, query: HybridQuery) -> Result<Vec<SearchResult>>;
    pub fn filter_by_importance(&self, results: Vec<SearchResult>, threshold: f32, now_tx: TxId) -> Vec<SearchResult>;
}

// Verknüpfungen & A-MEM (relate.rs)
impl<S, V> Collection<S, V> {
    pub async fn relate(&self, from, to, label) -> Result<()>;
    pub async fn relate_bidirectional(&self, from, to, label) -> Result<()>;
    // A-MEM: link_memories, traverse_links (ADR-038 — in Implementierung)
}

// Wartung (maintenance.rs)
impl<S, V> Collection<S, V> {
    pub async fn repair(&self) -> Result<()>;                              // repair_on_open
    pub async fn stats(&self) -> Result<VectorIndexStats>;
    pub async fn reap_expired_documents(&self, max_expired: usize) -> Result<usize>;
    pub async fn trigger_reaper(&self) -> Result<usize>;
    pub async fn evaluate_importance_with_llm(&self, id, ollama_client, model) -> Result<ImportanceScore>;
    pub async fn run_community_detection(&self) -> Result<Vec<CommunityAssignment>>;
    pub async fn run_community_detection_with_config(&self, config) -> Result<...>;
    pub async fn get_community(&self, entity_id: EntityId) -> Result<Option<u64>>;
    pub async fn drop_collection(&self) -> Result<()>;                     // löscht alle Daten
}

// Transaktions-Hilfsmethoden (tx.rs)
impl<S, V> Collection<S, V> {
    pub fn allocate_tx(&self) -> Result<TxId>;     // AtomicU64 inkrementiert
    pub fn begin_transaction(&self) -> Result<DbTransaction<S, V>>;
}
```

### 9.3 DbTransaction

```rust
pub struct DbTransaction<S: StorageEngine, V: VectorIndex = HnswIndex> { ... }
impl<S, V> DbTransaction<S, V> {
    pub fn new(collection: Collection<S, V>, tx_id: TxId) -> Self;
    pub fn record_keys(&self, forward: Vec<u8>, reverse: Vec<u8>, doc_id: DocId);
    pub fn stage_text_insert(&self, doc_id: DocId, text: String);
    pub fn stage_text_delete(&self, doc_id: DocId);
    pub fn stage_graph_entity(&self, entity: Entity);
    pub fn stage_graph_edge(&self, edge: Edge);
    pub fn stage_graph_entity_delete(&self, entity_id: EntityId);
    pub fn stage_graph_edge_delete(&self, from: EntityId, to: EntityId);
    pub async fn commit(self) -> Result<()>;   // atomarer Multi-Index Commit
    pub async fn rollback(self) -> Result<()>;
}
```

### 9.4 Fusion (`fusion.rs`)

```rust
pub fn reciprocal_rank_fusion(result_sets: Vec<Vec<SearchResult>>, max_results: usize) -> Vec<SearchResult>;
pub fn weighted_reciprocal_rank_fusion(
    result_sets: Vec<(String, Vec<SearchResult>, f32)>,
    max_results: usize,
) -> Vec<SearchResult>;
// k=60 (Cormack et al., 2009). Fusioniert Ränge, KEINE rohen Scores.
// INVARIANTE: Niemals eine dritte execute_rrf()-Funktion anlegen.
```

### 9.5 Kontext-Management

```rust
// ContextManager — Token-Budget-Trimmer
pub struct ContextManager { budget: TokenBudget, relevance_threshold: f32 }
impl ContextManager {
    pub fn new(budget: TokenBudget) -> Self;
    pub fn with_defaults() -> Self;
    pub fn set_relevance_threshold(&mut self, threshold: f32);
    pub fn relevance_threshold(&self) -> f32;
    pub fn prepare_context(&self, chunks: Vec<ContextChunk>) -> Result<ContextWindow>;
    pub fn estimate_tokens(text: &str) -> usize;   // Heuristik (chars / 4)
}

pub struct SpatialFence { region: String }
impl SpatialFence {
    pub fn new(region: impl Into<String>) -> Self;
    pub fn matches(&self, chunk: &ContextChunk) -> bool;
}
```

### 9.6 Context Compaction

```rust
pub enum CompactionStrategy {
    StatusToken,              // Ersetze Tool-Outputs durch StatusToken (Standard)
    LlmSummarize { max_input_chunks: usize },  // LLM-Zusammenfassung (ADR-032)
}

pub struct CompactedContext {
    pub chunks:         Vec<ContextChunk>,
    pub source_doc_ids: Vec<DocId>,    // Provenienz-IDs der komprimierten Chunks
    pub tokens_saved:   usize,
}

pub struct StatusToken { pub doc_id: DocId, pub status: String }

pub struct ContextCompactor { budget: TokenBudget, strategy: CompactionStrategy }
impl ContextCompactor {
    pub fn new(budget: TokenBudget, strategy: CompactionStrategy) -> Self;
    pub fn compact(&self, chunks: Vec<ContextChunk>) -> CompactedContext;
    pub async fn consolidate_via_llm(&self, chunks: &[ContextChunk], ollama: &OllamaClient) -> Result<CompactedContext>;
    // Fehler propagieren direkt → KEIN stiller Fallback auf StatusToken (ADR-032)
}
```

### 9.7 Multi-Step Query Engine

```rust
pub struct MultiStepConfig {
    pub max_rounds:     usize,        // Default: 3
    pub k_per_round:    usize,
    pub model_name:     String,       // Ollama-Modell-Name
}

pub struct MultiStepResult {
    pub results:     Vec<SearchResult>,
    pub rounds_used: usize,
    pub queries:     Vec<String>,    // alle genutzten Query-Varianten
}

pub struct MultiStepEngine<S: StorageEngine> {
    collection: Arc<Collection<S>>,
    config:     MultiStepConfig,
}
impl<S: StorageEngine> MultiStepEngine<S> {
    pub fn new(collection: Arc<Collection<S>>, config: MultiStepConfig) -> Self;
    pub async fn search(&self, initial_query, embedding, ollama: &OllamaClient) -> Result<MultiStepResult>;
}

pub trait QueryRewriter: Send + Sync {
    async fn rewrite(&self, query: &str, prev_results: &[SearchResult]) -> Result<String>;
}
pub struct OllamaQueryRewriter { client: OllamaClient, model: String }
```

### 9.8 Chunker

```rust
pub struct ChunkerConfig { pub chunk_size: usize, pub overlap: usize }

pub struct MarkdownChunker { config: ChunkerConfig }
impl MarkdownChunker {
    pub fn new(config: ChunkerConfig) -> Self;
    pub fn chunk(&self, text: &str) -> Vec<String>;
    pub fn chunk_with_metadata(&self, text, source: &str) -> Vec<(String, serde_json::Value)>;
}
```

---

## 10. Layer 3 — `memfuse-ollama`

**Zweck:** Ollama HTTP-Client, Embedder, ContextPrefixEngine (Anthropic Pattern).  
**`unsafe`:** nicht explizit verboten (pure Rust + HTTP)  
**LOC:** ~2.369

### 10.1 Öffentliche API

```rust
pub const DEFAULT_BASE_URL: &str;
pub const DEFAULT_EMBED_MODEL: &str;   // "nomic-embed-text"

// Ollama HTTP Client
pub struct OllamaConfig { pub base_url: String, pub timeout: Duration, pub max_retries: usize, pub model: String }
pub struct OllamaClient { config: OllamaConfig, ... }
impl OllamaClient {
    pub fn new(base_url: impl Into<String>) -> Self;
    pub fn with_config(config: OllamaConfig) -> Self;
    pub fn with_defaults() -> Self;
    pub fn base_url(&self) -> &str;
    pub fn config(&self) -> &OllamaConfig;
    pub async fn is_available(&self) -> bool;
    pub async fn embed_batch(&self, model, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
    pub async fn try_embed_batch(&self, model, texts: &[&str]) -> Result<Vec<Vec<f32>>>;   // mit Retry
    pub async fn list_models(&self) -> Result<Vec<String>>;
    pub async fn is_model_available(&self, model: &str) -> bool;
    pub async fn generate_text(&self, model, prompt) -> Result<String>;
    pub async fn try_generate_text(&self, model, prompt) -> Result<String>;  // mit Retry
    pub async fn generate(&self, model, prompt) -> Result<String>;
    pub async fn try_generate(&self, model, prompt) -> Result<String>;
}

// Validierende Hilfsfunktionen
pub fn sanitize_prompt_input(text: &str) -> String;           // XML-Injection Guard
pub fn validate_text_length(text, field_name) -> Result<()>;  // Max 10 MiB
pub fn validate_batch_size(count: usize) -> Result<()>;
pub fn validate_model_name(name: &str) -> Result<()>;
pub fn is_transient_network_error(err: &reqwest::Error) -> bool;
pub fn is_transient_error(e: &MemFuseError) -> bool;

// Embedder (implementiert TextEmbeddingEngine)
pub struct OllamaEmbedder { client: OllamaClient, model: String }
// TextEmbeddingEngine::embed, embed_batch

// Contextual Retrieval (Anthropic Pattern — 49% weniger Retrieval-Fehler)
pub struct ContextPrefixConfig {
    pub model:           String,
    pub max_prefix_tokens: usize,    // Default: 100
    pub max_doc_chars:   usize,      // Trunkierung für LLM-Prompt
}

pub struct ContextPrefixEngine { client: OllamaClient, config: ContextPrefixConfig }
impl ContextPrefixEngine {
    pub fn new(client: OllamaClient, config: ContextPrefixConfig) -> Self;
    pub async fn generate_prefix(&self, document: &str, collection_desc: &str) -> Result<String>;
    pub async fn generate_prefix_batch(&self, docs: &[(&str, &str)]) -> Result<Vec<String>>;
}

pub trait ContextPrefixer: Send + Sync {
    async fn prefix(&self, document: &str) -> Result<String>;
}

// Modell-Info
pub struct ModelInfo { pub name: String, pub size: u64, pub modified_at: String }

// Importance Scoring via LLM
pub async fn score_importance(client: &OllamaClient, model, text) -> Result<ImportanceScore>;

// Hilfsfunktionen
pub fn truncate_chars(s: &str, max_chars: usize) -> String;
pub fn truncate_prefix(s: &str, max_tokens, max_chars) -> String;
```

---

## 11. Layer 3 — `memfuse-embed`

**Zweck:** Optionaler In-Process ONNX Embedder + Cross-Encoder Reranker (Feature-gated).  
**`unsafe`:** `#![deny(unsafe_code)]` (ONNX C-FFI via `ort` bei `--features onnx`)  
**LOC:** ~1.022

### 11.1 Öffentliche API

```rust
pub const MAX_EMBED_BATCH_SIZE: usize = 10_000;

// Cross-Encoder Reranker (immer verfügbar, Passthrough ohne ONNX)
pub struct RerankConfig { pub top_k: usize, pub score_threshold: f32 }
pub struct RerankResult { pub id: String, pub score: f32, pub metadata: Option<...> }

pub struct CrossEncoderReranker { ... }
impl CrossEncoderReranker {
    pub fn new(config: RerankConfig) -> Result<Self, MemFuseError>;
    // Wenn kein ONNX: Passthrough (keine Verschlechterung, nur kein Reranking)
    // Mit ONNX (--features onnx): bge-reranker-base.onnx aus models/
}

// ONNX TextEmbedder (nur mit --features onnx)
#[cfg(feature = "onnx")]
pub struct TextEmbedderConfig { pub max_sequence_length: usize, pub pool_size: usize, pub expected_dim: Option<usize> }
#[cfg(feature = "onnx")]
pub struct TextEmbedder { ... }
// Implementiert TextEmbeddingEngine über spawn_blocking (verhindert Executor-Starvation)
```

---

## 12. Layer 3 — `memfuse-agent`

**Zweck:** Deterministischer `checkpoint → execute → commit → audit` Workflow-Loop.  
**`unsafe`:** `#![forbid(unsafe_code)]`  
**LOC:** ~3.134

### 12.1 Workflow State Machine

```
Idle → Running → (NodeEnd) → Completed
                → (Error)  → Failed
Crash-Recovery: CheckpointGuard::Drop → rollback_to_tx → replay_from()
```

### 12.2 Öffentliche API

```rust
// Validierungsfunktionen
pub fn validate_task_id(task_id: &str) -> Result<()>;    // max 256 Zeichen, kein '\0'
pub fn validate_node_id(node_id: &str) -> Result<()>;

// Agent-Kontext
pub enum AgentStatus { Idle, Running, Completed, Failed { error: String } }
pub struct AgentContext {
    pub task_id:        String,
    pub start_node:     String,
    pub status:         AgentStatus,
    pub memory:         HashMap<String, serde_json::Value>,
    pub token_budget:   TokenBudget,
    pub current_step:   usize,
    pub events:         Vec<BackgroundEvent>,
}
impl AgentContext {
    pub fn try_new(task_id, start_node, token_budget) -> Result<Self>;    // fallibel
    #[deprecated] pub fn new(task_id, start_node) -> Self;                // panikt → nicht nutzen
    pub fn attach_event(&mut self, event: BackgroundEvent);
    pub fn try_attach_event(&mut self, event: BackgroundEvent) -> Result<()>;
    // pub fn set_memory, get_memory, clear_memory, status(), ...
}

// StateGraph — deklarativer Workflow-Graph
pub enum NodeType { Start, Middle, End }
pub struct AgentNode { pub id: String, pub node_type: NodeType, pub metadata: serde_json::Value }
pub struct WorkflowEdge { pub from: String, pub to: String, pub condition: Option<String>, pub priority: u8 }

pub struct StateGraph { nodes: HashMap<String, AgentNode>, edges: Vec<WorkflowEdge> }
impl StateGraph {
    pub fn new() -> Self;
    pub fn try_add_node(&mut self, id, node_type, metadata) -> Result<()>;   // fallibel
    #[deprecated] pub fn add_node(&mut self, id, node_type, metadata);
    pub fn try_add_edge(&mut self, from, to, condition, priority) -> Result<()>;
    #[deprecated] pub fn add_edge(&mut self, from, to, condition, priority);
    pub fn get_node(&self, id: &str) -> Option<&AgentNode>;
}

// AgentTool — Plugin-Schnittstelle für Tool-Calls
#[async_trait]
pub trait AgentTool: Send + Sync {
    fn name(&self) -> &str;
    async fn execute(&self, ctx: &AgentContext, input: serde_json::Value) -> Result<StepResult>;
}

pub struct StepResult {
    pub node_id:         String,
    pub output:          serde_json::Value,
    pub tokens_consumed: usize,
    pub next_edge:       Option<String>,   // dynamische Edge-Auswahl
}

// OrchestratorEngine
pub struct OrchestratorEngine { storage: Arc<LsmStorage>, tools: HashMap<String, Box<dyn AgentTool>> }
impl OrchestratorEngine {
    pub fn new(storage: Arc<LsmStorage>) -> Self;
    pub fn from_db(db: &MemFuse) -> Self;
    pub fn try_register_tool(&mut self, tool: Box<dyn AgentTool>) -> Result<()>;
    #[deprecated] pub fn register_tool(&mut self, tool: Box<dyn AgentTool>);
    pub async fn run(&self, ctx: &mut AgentContext, graph: &StateGraph) -> Result<()>;
    pub async fn replay_from(&self, ctx: &mut AgentContext, identifier: &str) -> Result<()>;
    pub async fn checkpoint(&self, ctx: &AgentContext) -> Result<()>;
    pub async fn run_event_loop(&self, ctx: &mut AgentContext, graph: &StateGraph,
                                 event_sources: &mut Vec<Box<dyn EventSource>>) -> Result<()>;
}

// Audit-Log (immutabel, append-only)
pub struct AuditEntry { pub step: usize, pub node_id: String, pub output: Value, pub error: Option<String>, pub tokens: usize }
pub struct AuditLog<S: StorageEngine = LsmStorage> { collection: Arc<Collection<S>> }
impl<S> AuditLog<S> {
    pub fn new(collection: Arc<Collection<S>>) -> Self;
    // append, list, get_by_step, ...
}

// Event Sources
pub struct BackgroundEvent { pub source: String, pub payload: serde_json::Value }
impl BackgroundEvent {
    pub fn try_new(source, payload) -> Result<Self>;
    #[deprecated] pub fn new(source, payload) -> Self;
}

pub trait EventSource: Send + Sync {
    async fn poll(&mut self) -> Vec<BackgroundEvent>;
}

pub struct PollingDocumentEventSource<S: StorageEngine> { ... }
impl<S> PollingDocumentEventSource<S> {
    pub fn new(collection: Arc<Collection<S>>, poll_interval: Duration) -> Self;
    pub fn with_capacity(self, capacity: usize) -> Self;
    pub fn with_last_seen_seq(self, seq: u64) -> Self;
    pub fn poll_interval(&self) -> Duration;
}

pub struct VecEventSource { events: Vec<BackgroundEvent> }
impl VecEventSource {
    pub fn try_new(events: Vec<BackgroundEvent>) -> Result<Self>;
    #[deprecated] pub fn new(events: Vec<BackgroundEvent>) -> Self;
}
```

---

## 13. Layer 3 — `memfuse-router`

**Zweck:** Community-basiertes SLM-Routing. Leitet Queries an konfigurierte Small Language Model Endpunkte weiter.  
**`unsafe`:** nicht explizit  
**LOC:** ~510  
**⚠ DAG-Verletzung:** Importiert `memfuse_mcp::protocol::{JsonRpcRequest, JsonRpcResponse}` → Layer 3 → Layer 4

### 13.1 Öffentliche API

```rust
// SLM-Profil-Definition
pub struct SlmProfile {
    pub name:                 String,
    pub mcp_endpoint:         String,        // HTTP JSON-RPC 2.0 Endpunkt
    pub domain_communities:   Vec<u64>,      // Graph-Community-IDs
    pub token_budget:         TokenBudget,
    pub min_relevance_score:  f32,
}

// Routing-Ergebnis
pub struct RoutingDecision { pub profile: SlmProfile, pub context: ContextWindow }

// Router Engine
pub struct RouterEngine { collection: Arc<Collection<LsmStorage>>, profiles: Vec<SlmProfile> }
impl RouterEngine {
    pub fn new(collection: Arc<Collection<LsmStorage>>, profiles: Vec<SlmProfile>) -> Self;
    pub async fn route(&self, query_embedding: &[f32], query_text: &str) -> Result<RoutingDecision>;
    // Intern: hybrid_search → Community-Matching → Profile-Score → ContextManager::prepare_context
}

// HTTP-Dispatch an SLM-Endpunkt
pub async fn dispatch_to_slm(decision: &RoutingDecision) -> Result<String>;
// Sendet JsonRpcRequest an decision.profile.mcp_endpoint (reqwest, 30s Timeout)
```

---

## 14. Layer 3 — `memfuse-py`

**Zweck:** PyO3 Python FFI-Bindings. Null Panics über FFI-Grenze.  
**`unsafe`:** `#![forbid(unsafe_code)]` (PyO3 intern unsafe)  
**LOC:** ~1.007

### 14.1 Python-API (exportiert als `_memfuse` Modul)

```python
# Custom Exceptions
class MemFuseError(Exception): ...
class MemFuseIOError(MemFuseError): ...
class MemFuseIndexError(MemFuseError): ...
class MemFuseValueError(MemFuseError): ...
class MemFuseCryptoError(MemFuseError): ...
class MemFuseInternalError(MemFuseError): ...

class PyMemFuse:
    @staticmethod
    def open(path: str) -> PyMemFuse
    def collection(self, name: str) -> PyCollection
    def list_collections(self) -> list[str]
    def drop_collection(self, name: str) -> None
    def insert(self, id: str, embedding: np.ndarray, metadata: dict | None) -> None
    def upsert(self, id: str, embedding: np.ndarray, metadata: dict | None) -> None
    def get(self, id: str) -> dict | None
    def search(self, query: np.ndarray, k: int) -> list[dict]
    def hybrid_search(self, text: str, query: np.ndarray, k: int) -> list[dict]
    def delete(self, id: str) -> None
    def relate(self, from_id: str, to_id: str, label: str) -> None

class PyCollection:
    def insert(self, id, embedding, metadata) -> None
    def upsert(self, id, embedding, metadata) -> None
    def get(self, id) -> dict | None
    def search(self, query, k) -> list[dict]
    def hybrid_search(self, text, query, k) -> list[dict]
    def delete(self, id) -> None
    def relate(self, from_id, to_id, label) -> None
    # + weitere Methoden (scan, insert_text_only, ...)
```

**Intern:** Shared Tokio-Runtime via `OnceLock<Runtime>`. `block_on()` für alle Rust-async-Calls. GIL wird beim async-Aufruf freigegeben.

**Fehler-Mapping:** `MemFuseError` → passende Python-Exception-Subklasse.

---

## 15. Layer 4 — `memfuse-mcp`

**Zweck:** stdio JSON-RPC 2.0 MCP-Server (kein HTTP!), MCP Sandbox.  
**`unsafe`:** nicht explizit  
**LOC:** ~2.385  
**Transport:** ausschließlich stdin/stdout (ADR-010)

### 15.1 Protokoll (`protocol.rs`)

```rust
pub struct JsonRpcRequest {
    pub jsonrpc: String,  // "2.0"
    pub id:      Option<serde_json::Value>,
    pub method:  String,
    pub params:  serde_json::Value,
}

pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id:      Option<serde_json::Value>,
    pub result:  Option<serde_json::Value>,
    pub error:   Option<JsonRpcError>,
}
impl JsonRpcResponse {
    pub fn ok(id, result: Value) -> Self;
    pub fn err(id, code: i32, message) -> Self;
    pub fn from_error(id, err: McpError) -> Self;
}

pub struct JsonRpcError { pub code: i32, pub message: String, pub data: Option<Value> }

pub enum McpError { ParseError, InvalidRequest, MethodNotFound, InvalidParams, InternalError }
impl McpError {
    pub fn parse_error(msg) -> Self;
    pub fn invalid_request(msg) -> Self;
    pub fn method_not_found(msg) -> Self;
    pub fn invalid_params(msg) -> Self;
    pub fn internal_error(msg) -> Self;
    pub fn code(&self) -> i32;   // -32700, -32600, -32601, -32602, -32603
}
```

### 15.2 MCP-Tools (implementierte Methoden)

| Method | Beschreibung |
|---|---|
| `tools/list` | Listet alle verfügbaren Tools |
| `memfuse_insert` | Dokument einfügen (mit Chunking via MarkdownChunker) |
| `memfuse_search` | Vektorsuche (k ≤ MAX_SEARCH_K) |
| `memfuse_search_text` | BM25-Volltext-Suche |
| `memfuse_hybrid_search` | 4-Signal Hybridsuche |
| `memfuse_get` | Dokument abrufen |
| `memfuse_delete` | Dokument löschen |
| `memfuse_relate` | Relation zwischen Dokumenten anlegen |
| `memfuse_list_collections` | Collections auflisten |
| `memfuse_create_collection` | Collection erstellen |

### 15.3 Bounds & Guards

```rust
pub const MAX_RPC_BYTES: usize = 16 * 1024 * 1024;   // 16 MiB max Nachrichtengröße
pub const MAX_SEARCH_QUERY_BYTES: usize = 64 * 1024;  // 64 KiB max Query

pub async fn read_line_bounded<R: AsyncBufRead + Unpin>(
    reader: &mut R, buf: &mut String, max_bytes: usize,
) -> std::io::Result<usize>;
// Verbraucht Überlängen-Zeilen ohne Heap-Allokation
```

### 15.4 MCP Sandbox (`sandbox.rs`)

```rust
pub enum ToolCategory { Read, Write, Execute, Sensitive }

pub struct SandboxPolicy {
    pub allow_db_reads:      bool,    // Default: true
    pub allow_db_writes:     bool,    // Default: false (opt-in)
    pub allow_code_exec:     bool,    // Default: false
    pub allow_sensitive:     bool,    // Default: false
    pub tool_timeout_secs:   u64,     // Default: 30
}

// AES-256-GCM-SIV verschlüsselter, Zeroized-bei-Drop Tool-Output
pub struct VolatileToolResult { ciphertext: Vec<u8>, nonce: [u8; 12] }
impl VolatileToolResult {
    pub fn encrypt(plaintext: &[u8], key: &CryptoKey) -> Result<Self>;
    pub fn decrypt(&self, key: &CryptoKey) -> Result<zeroize::Zeroizing<Vec<u8>>>;
}

pub struct McpSandbox { policy: SandboxPolicy, key: CryptoKey, volatile_store: RwLock<HashMap<...>> }
impl McpSandbox {
    pub fn new(policy: SandboxPolicy) -> Result<Self>;
    pub fn policy(&self) -> &SandboxPolicy;
    pub fn validate_tool_call(&self, method: &str, params: &Value) -> Result<()>;
    pub fn classify_method(method: &str) -> ToolCategory;
    pub async fn execute_with_timeout<F, T, E>(&self, fut: F, timeout_secs: u64) -> Result<T, E>;
    pub fn store_volatile(&self, key: &str, output: &[u8]) -> Result<()>;   // verschlüsselt
    pub fn get_volatile(&self, key: &str) -> Result<Option<Zeroizing<Vec<u8>>>>;
    pub fn encode(bytes: [u8; 32]) -> String;   // Hex-Kodierung
}
```

---

## 16. Layer 4 — `memfuse-tauri`

**Zweck:** Desktop-Applikation "MemFuse Brain" (Tauri/WebView).  
**`unsafe`:** nicht explizit  
**LOC:** ~2.826

### 16.1 Tauri-Commands (invoke_handler)

| Command | Signatur (Rust) | Beschreibung |
|---|---|---|
| `open_database` | `(path: String) -> Result<(), String>` | DB öffnen, AppState initialisieren |
| `list_collections` | `() -> Result<Vec<String>, String>` | Collections auflisten |
| `create_collection` | `(name: String) -> Result<(), String>` | Collection anlegen |
| `drop_collection` | `(name: String) -> Result<(), String>` | Collection löschen |
| `ingest_file` | `(path: String, collection: String) -> Result<IngestReport, String>` | Datei ingesten |
| `ingest_folder` | `(path: String, collection: String) -> Result<Vec<IngestReport>, String>` | Ordner ingesten |
| `hybrid_search` | `(text: String, collection: String, k: usize) -> Result<Vec<SearchResult>, String>` | Suche |
| `chat_with_rag` | `(message: String, collection: String) -> Result<String, String>` | RAG-Chat |
| `list_ollama_models` | `() -> Result<Vec<String>, String>` | Ollama-Modelle auflisten |
| `run_regex_transform` | `(pattern, flags, replacement, input) -> Result<String, String>` | Regex-Transform |
| `run_bulk_regex_transform` | `(pattern, flags, replacement, inputs) -> Result<Vec<String>, String>` | Bulk |
| `validate_regex_pattern` | `(pattern, flags) -> Result<bool, String>` | Regex validieren |

### 16.2 Module

```
memfuse-tauri::
├── commands/       → Tauri-Command-Handler (mod.rs, transform.rs)
├── ingestion/      → Ingestion-Pipeline (pipeline.rs, email.rs, pdf, markdown)
├── ollama/         → OllamaBridge (localhost-Wrapper)
└── state/          → AppState (Arc<MemFuse>, regex_semaphore, embedder, ...)
```

### 16.3 AppState

```rust
pub struct AppState {
    pub db:               RwLock<Option<Arc<MemFuse>>>,
    pub embedder:         RwLock<Option<Arc<dyn TextEmbeddingEngine>>>,
    pub regex_semaphore:  Arc<Semaphore>,   // MAX_CONCURRENT_REGEX_OPS = 8 (ADR-014)
}
```

### 16.4 Regex-Sicherheitsregeln (ADR-014)

```
MAX_REGEX_INPUT_BYTES = 1 MiB (Standard) / 64 KiB (komplexe Patterns)
REGEX_TIMEOUT = 5s (Sicherheitsnetz, kein ReDoS-Schutz — regex-Crate ist NFA-basiert)
MAX_CONCURRENT_REGEX_OPS = 8 (Semaphore)
Engine: regex v1.x (NFA, kein Backtracking, kein Lookahead/Backreference → O(n) garantiert)
```

---

## 17. Crate-übergreifende Datenflüsse

### 17.1 Insert-Pfad (Vollständig)

```
Nutzer/Ollama → MemFuse::insert()
  → Collection::insert()
    ├── collection.allocate_tx() → TxId
    ├── insert_lock.lock()       → Serialisierung
    ├── DocId::from_key()        → BLAKE3 Hash
    ├── check_doc_id_collision() → Reverse-Lookup (TOCTOU-sicher im insert_lock)
    ├── LsmStorage::put(tx, forward_key, doc_bytes)   → TxBuffer staging
    ├── LsmStorage::put(tx, reverse_key, meta_bytes)
    ├── HnswIndex::insert(tx, doc_id, &embedding)     → TxBuffer staging
    ├── InvertedIndex::insert(tx, doc_id, &text)      → TxBuffer staging
    ├── CsrGraph::add_entity(tx, entity)              → Entity-Extraction
    ├── CsrGraph::add_edge(tx, edge)                  → Relation-Extraktion
    └── DbTransaction::commit()
          ├── LsmStorage::commit(tx_id)  → WAL-Schreib + MemTable
          ├── HnswIndex::commit(tx_id)   → TxBuffer drain → HNSW
          ├── InvertedIndex::commit(tx_id)
          └── CsrGraph::commit(tx_id)   → CSR compact() wenn nötig
```

### 17.2 Hybrid-Search-Pfad (4 Signale)

```
Query → Collection::hybrid_search_with_strategy(text, embedding, k, weights, filter, strategy, community)
  ├── Signal 1: HnswIndex::search(embedding, k*2) → Vec<ScoredDocument>   (Vektor)
  ├── Signal 2: InvertedIndex::search(text, k*2)  → Vec<ScoredDocument>   (BM25)
  ├── Signal 3: CsrGraph::traverse(entities, hops) | ppr(seeds)            (Graph)
  │             → Vec<(EntityId, f32)>
  ├── Signal 4: MetadataFilter::matches()          → bool-Filter auf S1/S2 Ergebnis
  │
  ├── fusion::weighted_reciprocal_rank_fusion([S1, S2, S3], k) → Vec<SearchResult>  (RRF, k=60)
  │
  ├── [Optional] CrossEncoderReranker::rerank()   → neu-geordnete Vec<SearchResult>
  │
  ├── [Optional] filter_by_importance(results, threshold, now_tx)  → gefilterte Liste
  │
  └── Vec<SearchResult>
```

### 17.3 MCP-Request-Pfad

```
stdin (Claude Desktop) → read_line_bounded() → serde_json::from_str::<JsonRpcRequest>()
  → McpSandbox::validate_tool_call()
  → handle_request(method, params)
      ├── MemFuse::hybrid_search() | insert() | ...
      └── JsonRpcResponse::ok(result) | ::from_error(err)
  → serde_json::to_string() → stdout
```

### 17.4 Agent-Workflow-Pfad

```
OrchestratorEngine::run(ctx, graph)
  LOOP:
    1. CheckpointGuard::for_agent_step(storage, tx)  → RAII-Guard (rollback bei Drop)
    2. graph.get_node(ctx.current_node) → AgentNode
    3. AgentTool::execute(ctx, input) → StepResult
    4. AuditLog::append(step_result)
    5. CheckpointGuard::commit()
    6. ctx.advance_to(next_node)
  END wenn NodeType::End
  
  Bei Panic/Crash:
    CheckpointGuard::drop() → LsmStorage::rollback_to_tx()
    Restart: OrchestratorEngine::replay_from(checkpoint_name)
```

---

## 18. Bekannte Schnittstellen-Besonderheiten & Fallstricke

### 18.1 TxId-Ursprung-Invariante (KRITISCH)

`TxId` darf **ausschließlich** aus einer dieser Quellen stammen:
1. `Collection::allocate_tx()` → atomarer `AtomicU64` Inkrement (Bereich `[1, ~10^12]`)
2. `TxId::INTERNAL_BASE` aufwärts (`u64::MAX - 1_000_000`) — für System-Transaktionen

❌ **Verboten:** `SystemTime::now().as_nanos() as u64` (~`1.7×10^18`) — korrumpiert `rollback_to_tx()` Kausalordnung.

### 18.2 TOMBSTONE_BIT-Disziplin (KRITISCH)

In allen Pfaden, die `max_seq` aus SSTables oder WAL lesen (rollback_to_tx, Startup-Recovery), **MUSS** vor Vergleichen und Zuweisungen maskiert werden:
```rust
let clean_seq = raw_seq & !TOMBSTONE_BIT;
```
Bit 63 ist ausschließlich ein Tombstone-Flag in Datensätzen — kein numerischer Sequenznummer-Bestandteil.

### 18.3 CapabilityUnsupported Default-Implementierungen

Folgende Trait-Methoden geben standardmäßig `Err(MemFuseError::CapabilityUnsupported)` zurück. Implementoren **MÜSSEN** sie überschreiben wenn die Fähigkeit benötigt wird:
- `StorageEngine::scan_prefix_at()` → capability: `"snapshot_read_at"`
- `VectorIndex::search_at()` → capability: `"snapshot_read_at"`
- `VectorIndex::search_filtered()` → capability: `"vector_filtered_search"` (wenn Filter ≠ None)
- `TextIndex::search_at()` → capability: `"snapshot_read_at"`
- `GraphIndex::traverse_at()` → capability: `"graph_traverse_at"`
- `GraphIndex::traverse_at_time()` → capability: `"graph_traverse_at_time"`
- `GraphIndex::personalized_page_rank()` → capability: `"graph_ppr"`

### 18.4 DocId-Kollision (ADR-016)

`DocId::from_key()` trunkiert BLAKE3 auf 64 Bit. Bei ~2^32 Keys besteht ~50% Kollisionswahrscheinlichkeit (Geburtstags-Paradoxon). `Collection::insert_op()` führt eine **Kollisionsprüfung via Reverse-Lookup innerhalb des `insert_lock`** durch. Kollision → `MemFuseError::Internal`.

### 18.5 Graph-Snapshot-Isolation (ADR-024)

Der CSR-Graph (Signal 3) operiert **auf dem aktuellen In-Memory-Zustand** — er ist **nicht** MVCC-snapshot-isoliert wie LSM-Store und HNSW. `traverse_at()` wirft `CapabilityUnsupported`. Nur `traverse_at_time()` ist via bi-temporaler Edge-Filterung (`valid_from`/`valid_to`) implementiert.

### 18.6 Lock-Hierarchie (Deadlock-Prävention)

```
MemFuse-Level:  collections (RwLock) → embedder (RwLock)
Collection:     insert_lock (Mutex)  → embedder (RwLock)
CsrGraph:       inner (RwLock) — darf NICHT über .await Punkte gehalten werden
SessionBranchTree: nodes → edges → active_head (strikte Reihenfolge)
```

### 18.7 Ollama-Laufzeit-Abhängigkeit (ADR-008)

`memfuse-ollama` erfordert einen **laufenden Ollama-Prozess**. Ohne Ollama schlagen alle Embedding- und LLM-Calls fehl. `is_available()` prüft Erreichbarkeit. `try_embed_batch()` / `try_generate_text()` beinhalten automatische Retry-Logik für transiente Netzwerkfehler.

### 18.8 WAL-Abwärtskompatibilität (ADR-029)

WAL-Format V3 (`MFW3` Header) ist aktuell. V1 und V2 werden beim Öffnen transparent zu V3 migriert. Neue Installations erstellen ausschließlich V3-Dateien. `tx_id` ist Teil der HMAC-Kette in V3 (verhindert tx_id-Tampering).

### 18.9 MAX_SEARCH_K-Enforcement

`MAX_SEARCH_K = 1000` wird in `HybridQueryBuilder::build()` und in der MCP-Schicht enforced. Der Literal `1000` darf **nirgendwo** im Workspace dupliziert werden — immer `memfuse_core::MAX_SEARCH_K` importieren.

### 18.10 Aktuelle DAG-Verletzung (TODO P0)

`memfuse-router` (Layer 3) importiert `memfuse_mcp::protocol::*` (Layer 4). **Fix:** `JsonRpcRequest` und `JsonRpcResponse` nach `memfuse_core::ipc` verschieben. Dann entfällt die `memfuse-mcp`-Abhängigkeit aus `memfuse-router`.

---

*Dieses Dokument wurde aus dem Repository-HEAD (ba861c68) durch vollständige Quellcode-Analyse generiert.*  
*Alle Signaturen sind verifiziert — keine Spekulation, kein Plan-Stand.*
