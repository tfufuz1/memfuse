# Tri-Hybrid Retrieval (RRF-Fusion & Query Engine) — Integration Guide für MemFuse

## 1. Technischer Hintergrund & Synergie
MemFuse implementiert ein 4-Signal-Hybrid-Retrieval (Vektor, BM25-Volltext, CSR-Graph, Metadaten-Filter) und fusioniert Teilergebnisse via **Reciprocal Rank Fusion (RRF)**. In der bisherigen MemFuse-Implementierung existierten jedoch getrennte Pfade und heterogene Formeln zwischen experimentellen Beispielen und dem Core-Engine-Pfad.

**Project Chimera** besitzt eine produktionserprobte, mathematisch konsistente Query-Engine:
- **Standardisierte RRF-Formel mit k=60:**
  $$\text{Score}(d) = \sum_{i=1}^M w_i \cdot \frac{1}{k + \text{rank}_i(d)}$$
  Die Glättungskonstante $k=60$ sorgt dafür, dass niedrigere Ränge kontrolliert abklingen, ohne Ausreißer überzubewerten.
- **AHashMap-basierte O(K) Fusion:** Schnelle In-Memory-Akkumulierung ohne redundante Heap-Klone.
- **Query Planner mit Short-Circuit Pruning:** Wenn ein Metadaten- oder Geofilter 0 Treffer liefert, wird die teure HNSW-Vektorsuche und Graph-Traversierung sofort abgebrochen.

## 2. Extrahierte Chimera-Komponenten

| Datei | Quelle | Relevanz für MemFuse |
|:---|:---|:---|
| [`fusion.rs`](./fusion.rs) | `chimera-query/src/fusion.rs` | Produktionserprobte gewichtete RRF-Engine mit $k=60$, deterministischer Sortierung und AHashMap-Akkumulator |
| [`planner.rs`](./planner.rs) | `chimera-query/src/planner.rs` | Query-Planner mit Filter-Pushdown und Short-Circuit Pruning |
| [`hybrid.rs`](./hybrid.rs) | `chimera-query/src/hybrid.rs` | Pipeline-Orchestrierung über heterogene Index-Engines |
| [`reciprocal_rank_fusion.md`](./reciprocal_rank_fusion.md) | `docs/concepts/reciprocal_rank_fusion.md` | Tiefenanalyse und mathematische Begründung für RRF |
| [`09_query_engine.md`](./09_query_engine.md) | `docs/architecture/09_query_engine.md` | Vollständige Architektur-Dokumentation der Chimera Query Engine |

## 3. Kern-Code-Auszug: Weighted RRF Engine
Aus [`fusion.rs`](./fusion.rs):
```rust
pub fn fuse_weighted(&self, lists: Vec<(Vec<ScoredDocument>, f32)>) -> Vec<ScoredDocument> {
    if lists.is_empty() {
        return Vec::new();
    }

    let estimated_docs = lists.iter().map(|(l, _)| l.len()).sum();
    let mut scores: AHashMap<DocId, f32> = AHashMap::with_capacity(estimated_docs);
    let mut docs: AHashMap<DocId, ScoredDocument> = AHashMap::with_capacity(estimated_docs);

    for (list, weight) in lists {
        for (rank, doc) in list.into_iter().enumerate() {
            // RRF Formula: weight * (1.0 / (k + rank + 1.0))
            let rrf_score = weight * (1.0 / (self.config.k + (rank as f32) + 1.0));
            *scores.entry(doc.doc_id).or_insert(0.0) += rrf_score;

            if !docs.contains_key(&doc.doc_id) {
                docs.insert(doc.doc_id, doc);
            }
        }
    }

    let mut results: Vec<ScoredDocument> = Vec::with_capacity(scores.len());
    for (doc_id, score) in scores {
        if let Some(mut doc) = docs.remove(&doc_id) {
            doc.score = score;
            results.push(doc);
        }
    }

    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));
    results
}
```

## 4. Implementierungsplan für MemFuse
1. Konsolidierung der `memfuse_db::fusion`-Funktionen auf Chimeras `RRFFusion` mit einheitlichem `k=60.0`.
2. Übernahme des Planner-Musters (`planner.rs`) in `memfuse-db::query_planner`, um Filter-Pushdown vor der HNSW-Suche zu garantieren.
