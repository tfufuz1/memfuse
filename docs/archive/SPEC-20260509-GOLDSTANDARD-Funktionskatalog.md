# MemFuse SAOS — Goldstandard Funktionskatalog
## Formale Spezifikation der Zukunftsfähigen Erweiterungen

> **Status:** PLANNING  
> **Basis:** SAOS-ARCHITECTURE.md + SAOS-ROADMAP.md + Goldstandard-Vision  
> **Erstellt:** 2026-05-08 | **Agent:** Context-Architekt (ANTIGRAVITY-Analyse)

---

## Übersicht: Die sieben Goldstandard-Funktionen

| ID | Funktion | Crate | Status | WP |
|----|---------|-------|--------|-----|
| GS-01 | 4-Signal Fusion API | memfuse-db | **DONE** | WP-6.1 |
| GS-02 | Declarative StateGraph API | memfuse-saos-agent | **DONE** | WP-6.2 |
| GS-03 | Autonomes Kontext-Management | memfuse-db | **DONE** | WP-6.3 |
| GS-04 | Multi-Agent Namespaces | memfuse-db | **DONE** | WP-6.4 |
| GS-05 | Morphologische Inferenz-Optimierung | memfuse-text | **DONE** | WP-6.5 |
| GS-06 | Air-Gap Deployment Profile | memfuse-py, memfuse-py | **WIP** | WP-6.6 |
| GS-07 | Kryptografische WAL-Verifikation | memfuse-store | **DONE** | WP-6.7 |

---

## GS-01 — 4-Signal Fusion API (Hybrid RAG)

### Zweck
Native Verschmelzung aller vier Retrieval-Signale in einer einzigen,
atomaren Query-Operation. Eliminiert die Notwendigkeit externer
Orchestrierungslogik für Multi-Modal-RAG.

### Funktionale Anforderungen

#### FR-GS01-001: Unified Query Interface
```rust
pub struct HybridQuery {
    /// Semantischer Vektor (HNSW) — normalized embedding
    pub vector: Option<Vec<f32>>,
    /// Keyword-Suche (BM25) — tokenisierter Query-String
    pub text: Option<String>,
    /// Kausale Traversierung (CSR-Graph) — Start-Node + Max-Hop-Depth
    pub graph_seed: Option<(NodeId, u8)>,
    /// Metadaten-Filter (Roaring Bitmap) — Filter-Expression
    pub metadata_filter: Option<FilterExpr>,
    /// Fusions-Gewichte [vector, text, graph, metadata] — muss 1.0 ergeben
    pub weights: FusionWeights,
    /// Top-K Ergebnisse
    pub limit: usize,
    /// Maximale Latenz-Garantie (Soft-Limit)
    pub timeout_ms: Option<u64>,
}

pub struct FusionWeights {
    pub vector: f32,   // Default: 0.4
    pub text: f32,     // Default: 0.3
    pub graph: f32,    // Default: 0.2
    pub metadata: f32, // Default: 0.1
}

impl Collection {
    /// Atomare 4-Signal-Fusion Query
    /// Garantiert: Latenz < 50ms bei < 1M Einträgen (embedded)
    pub async fn hybrid_search(
        &self,
        query: HybridQuery,
    ) -> Result<Vec<ScoredEntry>, MemFuseError>;
}
```

#### FR-GS01-002: Reciprocal Rank Fusion (RRF)
- Ergebnisse der vier Signale werden via RRF-60 Score verschmolzen
- `rrf_score(r) = 1 / (60 + rank(r))`
- Tie-breaking: Metadaten-Filter hat Priorität (deterministische Ordnung)

#### FR-GS01-003: Partial Signal Queries
- Alle vier Signal-Felder sind optional
- Mindestens ein Signal muss gesetzt sein
- Single-Signal-Query entspricht klassischer Einzel-Suche (kein Overhead)

#### FR-GS01-004: Graph-Signal (CSR-Graph)
- CSR (Compressed Sparse Row) für speicher-effiziente Graph-Traversierung
- BFS bis Tiefe `max_hop` mit Score-Decay: `score * 0.7^hop`
- Max-Hop: 3 (über 3 Hops sinkt Relevanz unter Noise-Floor)

### Performance-Anforderungen
- Latenz P50: < 10ms bei 100K Einträgen
- Latenz P99: < 50ms bei 1M Einträgen
- Memory: Kein Full-Scan — alle Signale müssen Index-backed sein

---

## GS-02 — Declarative StateGraph API

### Zweck
Macht externe Orchestrierungs-Frameworks (LangGraph, LangChain, CrewAI)
überflüssig. Entwickler definieren Agenten-Workflows deklarativ in Python.
Die hochoptimierte Rust-Engine übernimmt Ausführung, State-Verwaltung und
Fehlerbehandlung.

### Funktionale Anforderungen

#### FR-GS02-001: Python-seitige Workflow-Definition
```python
from memfuse import StateGraph, Node, Edge, Condition

# Deklarativer Workflow — keine Ausführungslogik im User-Code
graph = StateGraph(name="research_agent", checkpoint=True)

graph.add_node(Node(
    id="retrieve",
    tool="memfuse.hybrid_search",
    params={"limit": 10, "weights": {"vector": 0.6, "text": 0.4}},
))

graph.add_node(Node(
    id="synthesize",
    tool="llm.complete",
    params={"model": "claude-sonnet-4-20250514"},
))

graph.add_edge(Edge(
    source="retrieve",
    target="synthesize",
    condition=Condition.always(),
))

graph.add_edge(Edge(
    source="synthesize",
    target="retrieve",
    condition=Condition.on_field("needs_more_context", True),
    max_cycles=3,  # Loop-Breaker
))

# Rust-Engine übernimmt ab hier — kein Python im Hot Path
result = await graph.run(input={"query": "Was ist SAOS?"})
```

#### FR-GS02-002: State-Garantien
- Jeder Node-Übergang ist eine atomare WAL-Transaktion
- State ist nach jedem Node-Abschluss persistent (kein Zustand verloren)
- Re-Start nach Fehler beginnt am letzten erfolgreichen Node (via WP-5.1)

#### FR-GS02-003: Cycle-Detection & Loop-Breaker
- Max-Cycles pro Edge konfigurierbar (Default: 5)
- Bei Überschreitung: `Err(MemFuseError::MaxCyclesExceeded { edge_id, cycles })`
- Cycle-History in State für Debugging zugänglich

#### FR-GS02-004: Parallelisierung
- Unabhängige Nodes (keine gemeinsame Eingabe-Kante) laufen parallel
- `tokio::spawn` pro parallelem Zweig
- Merge-Node wartet auf alle Eingaben (Join-Semantik)

---

## GS-03 — Autonomes Kontext-Management

### Zweck
Das System injiziert proaktiv den relevantesten Kontext in das LLM-Arbeits-
gedächtnis, bevor Inferenz stattfindet. Reduziert Token-Last und erhöht
Antwort-Qualität ohne Entwickler-Eingriff.

### Funktionale Anforderungen

#### FR-GS03-001: Small-to-Big Retrieval
```rust
pub struct ContextManager {
    collection: Arc<Collection>,
    budget: TokenBudget,
}

impl ContextManager {
    /// Schritt 1: Kleine, präzise Chunks finden (Small Retrieval)
    /// Schritt 2: Übergeordnete Dokumente nachladen (Big Context)
    /// Schritt 3: Auf Token-Budget kürzen (Relevanz-gewichtet)
    pub async fn prepare_context(
        &self,
        query: &str,
        budget: TokenBudget,
    ) -> Result<ContextWindow, MemFuseError>;
}

pub struct TokenBudget {
    pub max_tokens: usize,    // Hard limit
    pub reserve_tokens: usize, // Für LLM-Output reserviert
}

pub struct ContextWindow {
    pub chunks: Vec<ContextChunk>,
    pub total_tokens: usize,
    pub truncated: bool, // True wenn Budget überschritten wurde
}
```

#### FR-GS03-002: Spatial Fencing
- Optional: Kontext auf geografische Region beschränken (für lokale Agenten)
- Filter via Metadaten-Feld `geo_region: String`
- Kombinierbar mit allen anderen Hybrid-Search-Signalen

#### FR-GS03-003: Adaptive Relevanz-Schwelle
- Chunks unter einem Relevanz-Score-Schwellwert werden ausgeschlossen
- Schwellwert adaptiert sich basierend auf Query-Qualität und verfügbarem Budget
- Konfigurierbar via `Collection::set_relevance_threshold(f32)`

---

## GS-04 — Multi-Agent Namespaces

### Zweck
Mehrere spezialisierte Agenten (Research, Code, Planning) teilen dieselbe
MemFuse-Instanz, ohne dass Kontext zwischen den Agenten durchsickert
(Context Bleeding).

### Funktionale Anforderungen

#### FR-GS04-001: Namespace-Isolation
```rust
pub struct Namespace {
    pub id: NamespaceId,
    pub name: String,
    pub isolation_level: IsolationLevel,
}

pub enum IsolationLevel {
    /// Vollständige Isolation — kein Lesen fremder Daten
    Strict,
    /// Geteiltes Lesen erlaubt — Schreiben isoliert
    SharedRead,
    /// Vollständig geteilt — nur logische Trennung (Default)
    Logical,
}

impl MemFuse {
    pub fn namespace(&self, id: &str) -> Result<NamespaceHandle, MemFuseError>;
    pub fn create_namespace(
        &mut self,
        name: &str,
        level: IsolationLevel,
    ) -> Result<NamespaceId, MemFuseError>;
}
```

#### FR-GS04-002: Cross-Namespace Queries (explizit)
- Expliziter API-Call für Cross-Namespace-Zugriff
- Erfordert `IsolationLevel::SharedRead` oder niedriger
- Logging aller Cross-Namespace-Zugriffe (für Audit-Trail)

#### FR-GS04-003: Namespace-Lifecycle
- Namespaces können archiviert werden (read-only)
- Archivierte Namespaces: Keine Writes, kein WAL-Append
- Restore aus Checkpoint möglich (via WP-5.1)

---

## GS-05 — Morphologische Inferenz-Optimierung

### Zweck
Senkung der Token-Last für europäische Sprachen durch sprachbewusste
Tokenisierung direkt auf Datenbankebene.

### Funktionale Anforderungen

#### FR-GS05-001: Morphem-Dekomposition
```rust
pub trait MorphologicalTokenizer: Send + Sync {
    /// Zerlegt "Bundesverfassungsgericht" in
    /// ["Bundes", "verfassungs", "gericht"]
    fn decompose(&self, token: &str) -> Vec<&str>;
    
    /// Sprache des Tokenizers
    fn language(&self) -> &str;
}

pub struct BM25MorphIndex {
    inner: InvertedIndex,
    tokenizer: Box<dyn MorphologicalTokenizer>,
}
```

#### FR-GS05-002: Compound-Splitting für Deutsch
- Wörterbuch-basiert + Häufigkeits-Statistik
- Fallback auf Standard-Tokenizer wenn keine Sprache erkannt

#### FR-GS05-003: Token-Reduktions-Metriken
- API gibt `token_reduction_ratio: f32` zurück
- Benchmark-Target: > 20% Token-Reduktion bei deutschen Fachtexten

---

## GS-06 — Air-Gap Deployment Profile

### Zweck
Vollständig offline-fähige MemFuse-Instanz für Sovereign AI deployments.

### Funktionale Anforderungen

#### FR-GS06-001: Offline-Embedding-Pipeline
```python
from memfuse import MemFuse, EmbeddingProvider

db = MemFuse(
    embedding=EmbeddingProvider.local(
        model_path="/models/e5-large-v2.onnx",
        runtime="ort",
    ),
    encryption=True,
    network=False,
)
```

#### FR-GS06-002: ONNX Runtime Integration
- `memfuse-py` Crate: Native ONNX Runtime Bindings (`ort`)
- Offizielle Unterstützung für:
  - `e5-small-v2` (66MB, 384-dim, multilingual) — Empfohlener Default für 8GB RAM
  - `bge-small-en-v1.5` (133MB, 384-dim)
  - `jina-embeddings-v2-small-en` (65MB, 512-dim)
- Kein separater Model-Server erforderlich. Model-Loading via `mmap` direkt aus dem App-Bundle.

#### FR-GS06-003: Netzwerk-Isolation-Guarantee
- `network=False` sperrt alle Socket-Calls
- Auditierbar: `memfuse verify-air-gap`

#### FR-GS06-004: Compliance-Export
- SPDX SBOM + Ed25519 Signing

---

## GS-07 — Kryptografische WAL-Verifikation

### Zweck
Deterministisches, kryptografisch verifizierbares Audit-Log.

### Funktionale Anforderungen

#### FR-GS07-001: Hash-Chaining
```rust
pub struct WalEntry {
    pub sequence: u64,
    pub timestamp: i64,
    pub entry_type: WalEntryType,
    pub payload: Vec<u8>,
    pub hash: [u8; 32],
    pub hmac: [u8; 32],
}

impl WalWriter {
    pub fn append(&mut self, entry_type: WalEntryType, payload: &[u8])
        -> Result<u64, MemFuseError>;
    pub fn verify_chain(&self) -> Result<VerificationReport, MemFuseError>;
}
```

#### FR-GS07-002: Time-Travel Debugging
- `WalReader::replay_to(sequence: u64)` rekonstruiert exakten State
- Kombiniert mit WP-5.1 (Checkpointing)

#### FR-GS07-003: WASM-Tool-Audit-Log
- Jede WASM-Sandbox-Ausführung schreibt WAL-Eintrag
- Privacy-by-Design: Nur Hashes, keine Payloads

---

## Entwicklungs-Reihenfolge

```
Sprint 5 — Foundation:    GS-04 (Namespaces) + GS-07 (Crypto WAL)
Sprint 6 — Intelligence:  GS-01 (4-Signal Fusion) + GS-05 (Morphologie)
Sprint 7 — Orchestration: GS-02 (StateGraph) + GS-03 (Kontext-Management)
Sprint 8 — Sovereign:     GS-06 (Air-Gap Deployment)
```
