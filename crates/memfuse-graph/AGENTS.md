# AGENTS.md — memfuse-graph
> Layer 1 | CSR-Graph, Entity-Relation Traversal, Session DAG | ~6100 LOC

## 1. Zweck & Architekturrolle

Wissensgraph-Engine (Signal 3 der 4-Signal-Fusion). Implementiert einen
Compressed Sparse Row (CSR) Graphen im Speicher, gekoppelt an LSM-Speicher
für Persistenz. Bietet bi-temporale Kanten, Personalized PageRank (PPR),
Community Detection sowie einen separaten `SessionBranchTree` für Agenten-Status.
Implementiert den `GraphIndex` Trait aus `memfuse-core`.

## 2. Modul-Karte

| Datei | Verantwortung |
|---|---|
| `lib.rs` | Modul-Deklaration, `#![deny(unsafe_code)]` |
| `csr.rs` | `CsrGraph` — Hauptstruktur, persistiert Entitäten & Kanten, GraphIndex-Impl |
| `ppr.rs` | `PprContext` — Personalized PageRank mit L1-Norm-Abbruch (ADR-026) |
| `community.rs` | `detect_communities` — Label Propagation Algorithmus (ADR-027) |
| `session_dag.rs` | `SessionBranchTree` — Agenten-Workflow-DAG, `AgentStateNode`, `DagEdge` |

## 3. Kritische Invarianten

### CSR-Persistenz-Präfixe
CSR-Entitäten und Kanten werden über den zugewiesenen `StorageEngine`
unter spezifischen LSM-Präfixen persistiert: `__graph:entity:` und `__graph:edge:`.
Bei LSM-Scans sind diese Präfixe system-intern und müssen vor normalen User-Daten verborgen werden.

### Bi-temporale Kanten (ADR-033)
Graph-Kanten unterstützen `valid_from` und `valid_to` basierend auf `TxId`.
Bei `insert_edge_direct_with_bitemporal_validity` ist darauf zu achten,
dass verfallene Kanten (wo `current_tx > valid_to`) bei Traversierungen ausgeblendet werden.

### TxId-Origin-Invariante (AGT-GRAPH-001)
`TxId` Argumente für Graphen-Updates MÜSSEN aus der Collection-eigenen `next_tx`-Sequenz 
oder aus dem `TxId::INTERNAL_BASE` Bereich (z.B. Checkpoint Replay) stammen.
Die Verwendung der aktuellen Wall-Clock (`SystemTime::as_nanos()`) korrumpiert die `rollback_to_tx()`-Kausalordnung!

### GraphEdge-Relation Synchronisation
In Layer 2 (Collection) **MUSS** bei `relate()`-Aufrufen synchron auch `graph_index.add_edge()`
aufgerufen werden. LSM-Metadaten allein reichen nicht für Graph-Traversal.

## 4. Public API Quick-Reference

```rust
// === CsrGraph (csr.rs) — Implementiert GraphIndex ===
pub struct CsrGraph { ... }
impl CsrGraph {
    pub async fn load_from_storage<S: StorageEngine>(storage: &S) -> Result<Self>;
    pub async fn persist_entity<S: StorageEngine>(&self, tx: TxId, entity: Entity, storage: &S) -> Result<()>;
    pub async fn persist_edge<S: StorageEngine>(&self, tx: TxId, edge: Edge, storage: &S) -> Result<()>;
    pub fn personalized_page_rank_with_context(&self, start: EntityId, ctx: &mut PprContext) -> Vec<(EntityId, f32)>;
}

// === Session DAG (session_dag.rs) ===
pub struct SessionBranchTree { ... }
impl SessionBranchTree {
    pub fn append_step(&self, parent: NodeIdx, node: AgentStateNode) -> Result<NodeIdx>;
    pub fn path_to_head(&self) -> Vec<AgentStateNode>;
}

// === Community Detection (community.rs) ===
pub async fn detect_communities(graph: &CsrGraph, config: CommunityDetectionConfig) -> Result<Vec<CommunityAssignment>>;
```

## 5. Anti-Patterns & LLM-Fallstricke

```rust
// ❌ FALSCH — SystemTime als TxId für Kanten:
let tx = TxId(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64);
graph.add_edge(tx, edge).await?;
// ✅ KORREKT — TxId vom Caller (Collection) übernehmen:
graph.add_edge(caller_provided_tx, edge).await?;

// ❌ FALSCH — Graph-Traversierung ohne Hop-Limit:
// ✅ KORREKT — max_hops oder PPR-Algorithmus verwenden.

// ❌ FALSCH — CsrGraph mit SessionBranchTree verwechseln:
// ✅ KORREKT — CsrGraph ist für Entitäten (Dokumente/Wissen), SessionBranchTree für Agenten-Status (Steps).
```

## 6. Concurrency & Lock-Hierarchie

`CsrGraph` nutzt intern `parking_lot::RwLock` für In-Memory CSR-Arrays.
Die I/O-Persistierung erfolgt komplett asynchron über den bereitgestellten `StorageEngine`.
Lese-Zugriffe (Traversierung, PPR) blockieren Writer nur extrem kurzfristig,
da Algorithmen auf Snapshots des Graphen oder optimierten read-only Views arbeiten.

## 7. Cross-Crate-Schnittstellen & DAG-Grenzen

- **Erlaubte Imports**: `memfuse-core` (L0)
- **Verbotene Imports**: `memfuse-store` (L1 Peer — wir importieren nur den `StorageEngine` Trait aus core), `memfuse-db` (L2)
- **Genutzt von**: `memfuse-db`, `memfuse-agent`

## 8. Relevante ADRs & Rules

| ADR/Rule | Relevanz |
|---|---|
| ADR-033 | Bi-temporaler Wissensgraph (TxId validity) |
| ADR-026 | Personalized PageRank (PPR) |
| ADR-027 | Community Detection Algorithmus |
| `rules/llm_protocol.md` | State-Transition Validation |
