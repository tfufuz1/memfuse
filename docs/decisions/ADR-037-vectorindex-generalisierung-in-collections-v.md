# ADR-037: VectorIndex-Generalisierung in Collection<S, V>


*   **Datum**: 2026-08-29
*   **Status**: ✅ Implementiert (2026-09-03)
*   **Entscheidung**: Die Datenstruktur `Collection<S: StorageEngine = LsmStorage>` in `crates/memfuse-db/src/collection.rs` wird generisch über den `VectorIndex`-Trait-Implementor erweitert: `Collection<S: StorageEngine = LsmStorage, V: VectorIndex = HnswIndex>`. Dadurch wird die starre Kopplung an `Arc<HnswIndex>` aufgehoben und die Nutzung alternativer Vektor-Indizes (wie z. B. `DiskAnnIndex` aus `memfuse-index`) ermöglicht.
*   **Alternativen**:
    - **Option A (Dynamischer Trait-Object Trait-Dispatch `Arc<dyn VectorIndex>)`**: Verworfen, da `VectorIndex` in manchen Pfaden dynamischen Trait-Funktions-Dispatch mit Performance-Overhead auf dem Hot-Path verbindet und die Typensicherheit bei konkreter Vektorindex-Instanziierung einbüßt.
    - **Option B (Status Quo belassen)**: Verworfen, da `DiskAnnIndex` als out-of-core Vektorindex vollständig implementiert ist, aber wegen der harten `Arc<HnswIndex>`-Typisierung in `Collection` ungenutzte technische Schuld darstellte.
*   **Begründung**: Die Verwendung eines generischen Typparameters mit Standard-Typ `V = HnswIndex` garantiert 100%ige Abwärtskompatibilität für alle bestehenden Aufrufer und Typ-Signaturen (wie `Collection<LsmStorage>`). Gleichzeitig wird die Entkopplung von der konkreten HNSW-Implementierung im `memfuse-db`-Crate vollzogen.
*   **Konsequenzen**:
    - `Collection` kann jetzt auch mit `DiskAnnIndex` instanziiert und betrieben werden (`Collection<LsmStorage, DiskAnnIndex>`).
    - `Collection::new` nimmt `index: Arc<V>` als Parameter auf; die Convenience-Funktion `Collection::with_hnsw` kapselt die bisherige HNSW-Konstruktion.
    - **Implementierungsnotiz (2026-09-03)**: `Collection<S, V>` ist jetzt generisch. Alle bestehenden Aufrufer nutzen weiterhin den Default `V = HnswIndex` ohne Typ-Annotationsänderung. `Collection<LsmStorage, DiskAnnIndex>` ist hinter `#[cfg(feature = "experimental-diskann")]` verfügbar.

---
