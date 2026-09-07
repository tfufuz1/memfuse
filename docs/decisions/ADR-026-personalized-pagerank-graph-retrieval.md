# ADR-026: Personalized PageRank (PPR) Graph Retrieval

*   **Datum**: 2026-08-28
*   **Status**: ✅ Final
*   **Entscheidung**:
    1. Implementierung von Personalized PageRank (PPR) als eigenständige, deterministische Power-Iterations-Methode auf der bestehenden CSR-Struktur (`CsrGraph`) in `crates/memfuse-graph/src/ppr.rs` ohne externe Bibliotheken (wie `petgraph`).
    2. Ergänzung von `PprConfig` und des Trait-Methoden-Contracts `personalized_page_rank` an `GraphIndex` in `memfuse-core`.
    3. Integration von PPR in `HybridQuery` (`memfuse-core`) und `Collection::hybrid_search_with_strategy` (`memfuse-db`) über die additiv wählbare `GraphTraversalStrategy` (`Hops` vs `PersonalizedPageRank`). Standardverhalten bleibt unverändert `GraphTraversalStrategy::Hops` (3 Hops BFS decay).
*   **Alternativen**:
    - **Option A (In-Tree `petgraph` Dependency)**: Verwendung von `petgraph` für PageRank. Verworfen, da `petgraph` eine Konvertierung/Kopie des CSR-Graphen erzwingen würde (Speicher- & Latenz-Overhead) und unkontrollierte Nicht-Determinismen einbringen könnte.
    - **Option B (`traverse` überschreiben)**: Ersetzung von BFS-Traversierung in `traverse()`. Verworfen, da BFS-Hop-Traversierung und PPR grundlegend unterschiedliche Retrieval-Semantiken besitzen (Hop-Distanz vs. Stationärverteilung eines Random-Walk-mit-Restart).
*   **Begründung**:
    - **Deterministische Konvergenz**: Die Power-Iteration auf dem CSR-Format verwendet eine explizite L1-Norm-Abbruchbedingung (`convergence_epsilon: 1e-6`) und eine harte Obergrenze (`max_iterations: 100`). Rank-Masse an Sackgassen-Knoten (Sackgassen / out-degree 0) wird gleichmäßig auf die Restart-Menge redistribuiert, um die stochastische Matrix-Eigenschaft zu wahren. Tie-Breaking über sekundäre Sortierung nach `EntityId` garantiert bitidentische Ergebnisse über mehrere Läufe.
    - **Zero-Panic / Zero-Hang**: Harte Abbruchschranken verhindern Endlosschleifen selbst auf pathologischen Graphen.
    - **Ruckfreie 4-Signal-Integration**: PPR ist als `GraphTraversalStrategy::PersonalizedPageRank` in `HybridQuery` und `Collection` nahtlos nutzbar und speist seine Ränge direkt in die Reciprocal Rank Fusion (RRF) ein.

---
