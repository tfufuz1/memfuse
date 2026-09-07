# ADR-027: Label Propagation für Community Detection & GraphRAG


*   **Datum**: 2026-08-27
*   **Status**: ✅ Final
*   **Kontext**: Für Phase 3 ("Community Detection & GraphRAG") wird eine Methode zur semantischen Clusterbildung von Wissensgraph-Knoten benötigt. Das Ergebnis (Community-Zuordnung pro EntityId) soll asynchron als Batch-Prozess berechnet, im Storage unter `__graph:community:<entity_id>` abgelegt und beim Retrieval gelesen werden.
*   **Entscheidung**:
    - Wahl des **Label-Propagation-Algorithmus (LPA)** anstelle von Louvain.
    - Vollständig deterministische Ausführung durch fixierten RNG-Seed für Knoten-Shuffling und ein striktes Tie-Breaking: Bei relativer oder absoluter Gleichheit von Label-Gewichten gewinnt das kleinstmögliche `EntityId` (numerischer `u64`-Wert).
    - Implementierung direkt auf der bestehenden `CsrGraph`-Struktur in `memfuse-graph::community` ohne zusätzliche externe Abhängigkeiten.
    - Persolidierung im LSM-Storage über `Collection::run_community_detection()` mit strenger TxId-Allokation (`self.allocate_tx()`).
    - Anbindung an das Retrieval über `HybridQuery::same_community_as`, welches Kandidaten derselben Community vor der RRF-Fusion filtert bzw. verstärkt.
*   **Alternativen**:
    - **Louvain-Algorithmus**: Louvain ist bei paralleler Ausführung ohne schwere Synchronisation nicht-deterministisch und erfordert komplexe Graph-Hierarchie-Strukturen.
    - **Echtzeit-Clustering bei jeder Query**: Zu hohe Latenz und Token-Kosten, widerspricht den Zero-Latency- und Sovereign-Core-Prinzipien.
*   **Begründung**: Label Propagation ist hochgradig speichereffizient, lässt sich nahtlos auf CSR-Arrays ausführen, ist ohne externe C/Rust-Dependencies umsetzbar und garantiert bei striktem Tie-Breaking 100%ige Reproduzierbarkeit und Zero-Panic-Sicherheit.
*   **Konsequenzen**:
    - Neue Datei `crates/memfuse-graph/src/community.rs`.
    - Neuer Subcommand `run-community-detection` in `xtask`.
    - Erweiterung von `HybridQuery` und `Collection::hybrid_search_ext`.

---
