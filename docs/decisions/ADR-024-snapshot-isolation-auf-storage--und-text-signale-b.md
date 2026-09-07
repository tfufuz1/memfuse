# ADR-024: Snapshot-Isolation auf Storage- und Text-Signale beschränkt (Vektor/Graph nicht snapshot-isoliert)


*   **Datum**: 2026-08-28
*   **Status**: ✅ Final
*   **Kontext**: Das Trait-Design in `memfuse-core::traits` definiert snapshot-isolierte Methoden `search_at` (`VectorIndex`, `TextIndex`, `StorageEngine`) und `traverse_at` (`GraphIndex`). Eine Quellcode-Analyse ergab, dass `scan_prefix_at` (`LsmStorage`) und `search_at` (`InvertedIndex`) voll snapshot-isoliert implementiert sind. `HnswIndex::search_at`, `DiskAnnIndex::search_at` und `CsrGraph::traverse_at` sind aktuell nicht überschrieben und liefern standardmäßig `Err(MemFuseError::PolicyViolation(...))` zurück. `Collection::hybrid_search()` verwendet für Vektor- und Graph-Signale die aktuellen in-memory Suchmethoden `search()` und `traverse()`, während Storage-Dokumenthydration und Textsuche über `snapshot_seq()` isoliert werden.
*   **Entscheidung**:
    - Es wird explizit dokumentiert, dass Snapshot-Isolation in MemFuse aktuell auf Storage- (LSM-Tree) und Text-Signale (BM25) beschränkt ist. Vektorsuche (`HnswIndex`, `DiskAnnIndex`) und Graph-Traversal (`CsrGraph`) operieren auf dem jeweils aktuellen In-Memory-Zustand.
    - Die Default-Fehlermeldungen in `VectorIndex::search_at` und `GraphIndex::traverse_at` werden präzisiert, um transparent auf ADR-024 zu verweisen: `"Snapshot isolation for vector/graph search is not yet implemented — tracked in ADR-024"`.
    - Sobald Snapshot-Isolation für In-Memory Vektor- und Graph-Strukturen implementiert wird, werden `HnswIndex::search_at`, `DiskAnnIndex::search_at` und `CsrGraph::traverse_at` überschrieben und in `Collection::hybrid_search()` angebunden.
*   **Alternativen**:
    - **Option A (Feature erzwingen)**: Sofortiges Re-Engineering von `HnswIndex` und `CsrGraph` zur vollständigen Node/Edge-Versionierung pro Sequence-Number. Verworfen wegen hohem Risiko komplexer Regressionen in den Kern-Traversierungs-Performanzpfaden ohne vorheriges Design-Review.
    - **Option B (Fail-silent belassen)**: Unveränderte Beibehaltung generischer Trait-Fehlermeldungen ohne Dokumentation. Verworfen, da dies das `CONSTITUTION.md`-Prinzip "No Silent Failures" und "Ehrliche Invarianten" verletzt.
*   **Begründung**: Option B bzw. Klärung via ADR-024 stellt sicher, dass Entwickler und Nutzer exakt wissen, welche Signale snapshot-isoliert sind (Storage + Text) und welche auf dem aktuellen In-Memory-Stand arbeiten (Vektor + Graph), ohne falsche API-Versprechungen zu machen.
*   **Konsequenzen**:
    - Aktualisierung der Invariantentabelle in `docs/ARCHITECTURE.md`.
    - Aktualisierung der Trait-Default-Fehlermeldungen in `crates/memfuse-core/src/traits.rs`.
    - Hinzufügen expliziter Integrationstests, die das dokumentierte Verhalten absichern.

---
---
