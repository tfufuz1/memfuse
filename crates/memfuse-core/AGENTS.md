# AGENTS.md — memfuse-core
> Layer 0 | Dependency Root: Typen, Traits, Fehlerbehandlung | ~8500 LOC

## 1. Zweck & Architekturrolle

Kernel-Fundament des gesamten Workspace. **Alle** anderen Crates hängen von `memfuse-core` ab.
Definiert die abstrakten Schnittstellen (Traits), Domain-Primitiven (IDs, Scores, Budgets)
und die einzige Fehler-Enum `MemFuseError`. Enthält **kein I/O, kein async, kein Netzwerk**
in den Typ-Modulen — ausschließlich reine Datenstrukturen und Trait-Contracts.

## 2. Modul-Karte

| Datei | Verantwortung |
|---|---|
| `lib.rs` | Re-Exports, `#![deny(unsafe_code)]`, `#![warn(missing_docs)]` |
| `error.rs` | `MemFuseError` Enum (einzige Error-Quelle im Workspace), `Result<T>` Alias |
| `error_dto.rs` | `MemFuseErrorDto` — FFI-safe Fehler-Repräsentation für Python/MCP Grenzen |
| `traits.rs` | **Kern-Trait-Hierarchie**: `StorageEngine`, `VectorIndex`, `TextIndex`, `GraphIndex`, `CheckpointCoordinator`, `TextEmbeddingEngine`, `Checkpoint` |
| `types/domain.rs` | `DocId`, `EntityId`, `TxId` (Newtypes über u64), `Entity`, `Edge`, `ScoredDocument`, `MemoryType`, `LinkRelation`, `MemoryLink`, `PprConfig` |
| `types/saos.rs` | `ContextChunk`, `ContextWindow`, `HybridQuery`, `HybridQueryBuilder`, `FusionWeights`, `ScoredEntry`, `GraphTraversalStrategy` |
| `types/budget.rs` | `TokenBudget`, `ResourceBudget`, `ResourceTracker`, `BudgetStrategy` |
| `types/importance.rs` | `ImportanceScore`, `DecayFunction`, `MemoryImportance` |
| `types/filter.rs` | `FilterExpr` — strukturierte Metadaten-Filterausdrücke |
| `seq_log.rs` | `SequenceLog` — Append-Only Sequenzlog für MVCC-Sichtbarkeit im HNSW |
| `snapshot.rs` | `SnapshotRegistry`, `SnapshotGuard` — MVCC Read-Isolation |
| `tx_buffer.rs` | `TxBuffer` — Sharded Transaction Staging mit Orphan Reaper, `IndexOp` |
| `ipc/` | JSON-RPC 2.0 Typdefinitionen für Inter-Crate-IPC (ADR-045) |

## 3. Kritische Invarianten

### Keine-I/O-Garantie
Layer 0 darf **niemals** Dateisystem-, Netzwerk- oder async-Operationen enthalten.
Typen und Traits definieren Contracts — Implementierungen leben in höheren Schichten.

### Einzige Error-Enum
`MemFuseError` ist die **einzige** Fehler-Enum im gesamten Workspace.
Keine crate-lokalen Error-Enums. Neue Varianten am Ende anfügen (Append-Only, Binary Compat).
`From`-Impls ausschließlich in `error.rs` — keine Wildcard-`From<E>` in anderen Modulen.

### Trait-Abwärtskompatibilität (Default-Impl-Pflicht)
Neue Trait-Methoden **MÜSSEN** eine Default-Implementierung haben.
Default-Impls für nicht unterstützte Features werfen standardisiertes `CapabilityUnsupported`.

### Trait-Default-Pflichttest
Für jedes `pub trait` mit Default-Methode **MUSS** ein Integrationstest existieren,
der beweist, dass die Default-Implementierung NICHT still greift
(siehe `capability_coverage` Testmodul in `traits.rs`).

### TxId-Bereiche
- **Collection-Sequenz**: `[1, ~10^12]` — via `collection.allocate_tx()`
- **Interner Systembereich**: `TxId::INTERNAL_BASE` (`u64::MAX - 1_000_000`) aufwärts — Checkpoint, WAL-Replay

## 4. Public API Quick-Reference

```rust
// === Kern-Traits (traits.rs) ===
trait StorageEngine: Send + Sync + 'static {
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;
    async fn put(&self, tx_id: TxId, key: &[u8], value: &[u8]) -> Result<()>;
    async fn commit(&self, tx_id: TxId) -> Result<()>;
    async fn scan_prefix_at(&self, prefix: &[u8], seq_no: u64) -> Result<Vec<(Vec<u8>, Vec<u8>)>>;
    // Default: CapabilityUnsupported
}

trait VectorIndex: Send + Sync + 'static {
    async fn insert(&self, tx: TxId, id: DocId, embedding: &[f32]) -> Result<()>;
    async fn search(&self, query: &[f32], k: usize) -> Result<Vec<ScoredDocument>>;
    async fn search_at(&self, query: &[f32], k: usize, seq_no: u64) -> Result<Vec<ScoredDocument>>;
    // Default: CapabilityUnsupported
}

trait TextIndex: Send + Sync + 'static {
    async fn search(&self, query: &str, k: usize) -> Result<Vec<ScoredDocument>>;
    async fn search_at(&self, query: &str, k: usize, seq_no: u64) -> Result<Vec<ScoredDocument>>;
    // Default: CapabilityUnsupported
}

trait GraphIndex: Send + Sync + 'static {
    async fn traverse(&self, start: EntityId, max_hops: usize) -> Result<Vec<(EntityId, f32)>>;
    async fn add_entity(&self, tx: TxId, entity: Entity) -> Result<()>;
    async fn add_edge(&self, tx: TxId, edge: Edge) -> Result<()>;
}

trait TextEmbeddingEngine: Send + Sync + 'static {
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
}

// === Domain-Primitiven (types/domain.rs) ===
pub struct DocId(pub u64);     // Blake3-Hash-Ableitung via DocId::from_key()
pub struct EntityId(pub u64);  // 1:1 Korrespondenz mit DocId für RRF-Hydrierung
pub struct TxId(pub u64);      // Monoton steigend, NIEMALS SystemTime!
```

## 5. Anti-Patterns & LLM-Fallstricke

```rust
// ❌ HALLUZINATION — MemFuseError varianten erfinden:
return Err(MemFuseError::GraphError("...".into())); // Existiert nicht!
// ✅ KORREKT:
return Err(MemFuseError::Internal("Graph: ...".into()));

// ❌ Typ-Duplikation — ContextChunk neu definieren:
struct ContextChunk { ... } // Existiert bereits in types/saos.rs!
// ✅ VOR jedem struct/enum:
// grep "<TYPNAME>" docs/TYPE_REGISTRY.md
// find crates/ -name "*.rs" | xargs grep "struct <TYPNAME>"

// ❌ Default-Impl vergessen bei neuem Trait-Method:
trait StorageEngine { async fn new_method(&self) -> Result<()>; }
// ✅ KORREKT — Default mit CapabilityUnsupported:
async fn new_method(&self) -> Result<()> {
    Err(MemFuseError::capability_unsupported("new_method", "Not implemented"))
}
```

## 6. Concurrency & Lock-Hierarchie

Keine Locks in memfuse-core selbst. `TxBuffer` nutzt `DashMap` (lock-free sharded HashMap).
`SnapshotRegistry` nutzt `parking_lot::Mutex` — nur ein Lock, keine Hierarchie-Regeln.

## 7. Cross-Crate-Schnittstellen & DAG-Grenzen

- **Erlaubte Imports**: Nur externe Crates (`serde`, `async_trait`, `thiserror`, `blake3`, `parking_lot`, `dashmap`)
- **Verbotene Imports**: Alle Workspace-Crates — Layer 0 hat **NULL** Workspace-Abhängigkeiten
- **Implementoren der Traits**:
  - `StorageEngine` → `LsmStorage` in `memfuse-store`
  - `VectorIndex` → `HnswIndex` in `memfuse-index`
  - `TextIndex` → `Bm25Scorer` in `memfuse-text`
  - `GraphIndex` → `CsrGraph` in `memfuse-graph`
  - `TextEmbeddingEngine` → `OllamaEmbedder` in `memfuse-ollama`

## 8. Relevante ADRs & Rules

| ADR/Rule | Relevanz |
|---|---|
| `rules/error-handling.md` | Variant Policy, From-Impls, append-only |
| `rules/llm_protocol.md` | Schleife 1: Read-Before-Write für Core-API-Signaturen |
| ADR-028 | TS: + SESSION: Pflichtfelder auf allen Tags |
| ADR-024 | Snapshot Isolation Defaults (CapabilityUnsupported) |
| `docs/TYPE_REGISTRY.md` | Typ-Kollisionsprüfung vor Neuanlage |
