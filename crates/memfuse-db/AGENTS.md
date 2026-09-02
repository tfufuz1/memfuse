# AGENTS.md — memfuse-db
> Layer 2 | Collection, 4-Signal-Fusion, Context Compaction | ~15000 LOC

## 1. Zweck & Architekturrolle

Orchestriert die vier Kern-Signale (LSM, Vector, Graph, BM25) zu einer unified
`Collection`. Beinhaltet die komplexe Logik für Reciprocal Rank Fusion (RRF),
Markdown-Chunking, Token-Budget-Management und LLM-gestützte Context Compaction.
Ist die primäre High-Level-API für lokale Agenten.

## 2. Modul-Karte

| Datei | Verantwortung |
|---|---|
| `lib.rs` | `#![deny(unsafe_code)]`, Lock-Hierarchie-Definition |
| `collection/` | `Collection`, Tx-Allokation (`next_tx`), Insert/Search-Routinen |
| `fusion.rs` | 4-Signal RRF (`reciprocal_rank_fusion`), Priorisierung |
| `context.rs` | `ContextManager`, `SpatialFence`, Relevanz-Thresholds, Token-Counting |
| `context_compaction.rs` | `ContextCompactor`, `ConsolidationSession` (LLM-Zusammenfassung) |
| `chunker.rs` | `MarkdownChunker` — Strukturiertes Aufteilen von Markdown-Dokumenten |
| `multistep.rs` | `MultiStepEngine`, `QueryRewriter` — LLM-gestützte iterative Suche |
| `transaction.rs` | `CommitIntent` — High-Level Transaktionssteuerung |
| `reaper.rs` | Background Tasks: Expiry Reaper, Orphan Reaper |

## 3. Kritische Invarianten

### TxId Generierung (AGT-DB-001)
`TxId` **MUSS IMMER** über `collection.allocate_tx().await` bezogen werden.
Es inkrementiert deterministisch den atomaren Zähler `next_tx`.
Niemals `SystemTime` verwenden (Kausalitätsbruch bei Graph & LSM)!

### 4-Signal Fusion Pipeline
Bei `collection.search()` werden 4 Engines asynchron parallel abgefragt:
Vector (HNSW), Text (BM25), Graph (PPR), Storage (LSM).
Die Ergebnisse MÜSSEN zwingend durch `reciprocal_rank_fusion` (bzw. gewichtet)
laufen, um Score-Verzerrungen aus unterschiedlichen Domänen auszugleichen.

### MarkdownChunker Pflicht
Agenten-Wissen besteht oft aus Markdown. Es **DARF NICHT** als einzelner gigantischer
String an die Embedding-Engine gegeben werden. Der `MarkdownChunker` ist Pflicht,
um Dokumente anhand von Headings (`#`, `##`) in sinnvolle semantische Chunks
(ca. 512 Tokens) zu splitten, wobei Parent-Headings kaskadiert werden.

### Lock-Hierarchie (Deadlock Prevention)
Wenn mehrere Komponenten gelockt werden müssen, gilt zwingend folgende Reihenfolge:
1. **`collections` (RwLock)**: Die Registry aller aktiven Collections (äußerster Lock).
2. **`insert_lock` (Mutex)**: Pro Collection, schützt Batch-Updates.
3. **`embedder` (RwLock)**: Lazy-Initialization des TextEmbeddingEngines.
*Jede Abweichung erzeugt Deadlocks unter Last.*

## 4. Public API Quick-Reference

```rust
// === Collection (collection/mod.rs) ===
pub struct Collection<S: StorageEngine> { ... }
impl<S> Collection<S> {
    pub async fn allocate_tx(&self) -> Result<TxId>;
    pub async fn insert_document(&self, tx: TxId, doc_id: DocId, markdown: &str) -> Result<()>;
    pub async fn search(&self, query: &HybridQuery) -> Result<ContextWindow>;
}

// === RRF & Context (fusion.rs, context_compaction.rs) ===
pub fn reciprocal_rank_fusion(lists: Vec<Vec<ScoredEntry>>) -> Vec<ScoredEntry>;
pub struct ContextCompactor { ... }
impl ContextCompactor {
    pub async fn consolidate_via_llm(&self, session: ConsolidationSession) -> Result<CompactedContext>;
}

// === Chunker (chunker.rs) ===
pub struct MarkdownChunker { ... }
impl MarkdownChunker {
    pub fn chunk(&self, doc_id: DocId, markdown: &str) -> Vec<ContextChunk>;
}
```

## 5. Anti-Patterns & LLM-Fallstricke

```rust
// ❌ FALSCH — SystemTime als TxId (verletzt Kausalität):
let tx = TxId(std::time::UNIX_EPOCH.elapsed().unwrap().as_nanos() as u64);
// ✅ KORREKT:
let tx = collection.allocate_tx().await?;

// ❌ FALSCH — Ganze Dokumente einbetten:
collection.insert_document(tx, doc_id, very_long_markdown).await?; // Chunking vergessen!
// ✅ KORREKT:
let chunks = chunker.chunk(doc_id, very_long_markdown);
for chunk in chunks { collection.insert_chunk(...).await?; }

// ❌ FALSCH — RRF Array-Out-Of-Bounds (bekannte Halluzination):
// ✅ KORREKT: RRF-Algorithmus darf keine festen Indexe annehmen (z.B. lists[0]),
// da manche Engines leere Ergebnisse liefern können.
```

## 6. Concurrency & Lock-Hierarchie

(Siehe Sektion 3)
Zusätzlich: LLM-gestützte Operationen (`consolidate_via_llm`, `MultiStepEngine::search`)
dauern Sekunden! Sie DÜRFEN NIEMALS unter einem aktiven `insert_lock` oder `RwLockReadGuard`
ausgeführt werden (Fehlerklasse 11).

## 7. Cross-Crate-Schnittstellen & DAG-Grenzen

- **Erlaubte Imports**: `memfuse-core` (L0), `memfuse-store`, `memfuse-index`, `memfuse-graph`, `memfuse-text` (alle L1), `memfuse-checkpoint`
- **Verbotene Imports**: `memfuse-agent` (L3), `memfuse-router` (L3), `memfuse-mcp` (L4)
- **Genutzt von**: Fast alle Layer 3/4 Crates

## 8. Relevante ADRs & Rules

| ADR/Rule | Relevanz |
|---|---|
| ADR-022 | RRF Algorithmus & Scoring-Normalisierung |
| ADR-023 | LLM-gestützte Context Compaction (ConsolidationSession) |
| `COMMON_LLM_ERRORS.md` | Fehler-Klasse 11: Lock-Guard über `.await` |
| `COMMON_LLM_ERRORS.md` | Fehler-Klasse 5: RRF Panic Out-of-Bounds |
