# ADR-013: DiskANN als experimentelles Feature (memfuse-index)

*   **Datum**: 2026-08-23
*   **Status**: ✅ Final
*   **Entscheidung**: Die Out-of-Core-Vektorsuche (DiskANN) im `memfuse-index` Crate wird als experimentell markiert und hinter dem Cargo-Feature `experimental-diskann` sowie `#[doc(hidden)]` verborgen. Sie wird (vorerst) nicht in die abstrahierte `VectorIndexBackend`-Schnittstelle des `memfuse-db`-Crates integriert.
*   **Alternativen**:
    - **Option A**: Volle Integration durch Refactoring der `VectorIndex`-Abstraktion und Anpassung der `memfuse-db::Collection`, um dynamisch zwischen HNSW und DiskANN zu wechseln.
*   **Begründung**: `memfuse-db::Collection` und `HnswIndex` sind aktuell extrem eng verzahnt (z.B. direkte Nutzung von `all_doc_ids_from_map()` in der Collection). Eine überhastete Integration würde die Architektur-Integrität und Snapshot-Isolation gefährden, da DiskANN derzeit `insert()` und `delete()` nicht vollständig (oder nur mit `Err`) implementiert. Option A hätte gravierende Umbauten am Kern-Datenfluss der Collection zur Folge gehabt. Das Verbergen von DiskANN schützt die Produktionspfade, lässt aber den Code für zukünftige Entwicklungen im Baum.
*   **Konsequenzen**:
    - `memfuse-db` nutzt HNSW weiterhin hartcodiert.
    - Endnutzer sehen die DiskANN-Funktionalität nicht in der öffentlichen API.

---
