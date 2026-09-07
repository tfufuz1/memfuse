# ADR-017: Explicit Authorization of `unsafe` Mmap in DiskANN (BEFUND AGT-AUDIT-002)

*   **Datum**: 2026-08-24
*   **Status**: ✅ Final
*   **Entscheidung**: Die generelle Architekturregel ("`unsafe` ist ausschließlich in `memfuse-index/src/distance.rs` erlaubt") wird für `memfuse-index/src/diskann.rs` und `memfuse-index/src/persistence.rs` erweitert. Ein expliziter `unsafe { Mmap::map(...) }`-Aufruf ist dort zulässig, MUSS aber zwingend durch einen `// SAFETY:`-Kommentar begründet sein, der die Validität des File-Deskriptors und der Längenprüfung belegt. Modulweite `#![allow(unsafe_code)]`-Attribute bleiben strengstens verboten.
*   **Alternativen**:
    - **Option A**: Refactoring auf sichere I/O-Methoden (z. B. pread) ohne Mmap. Verworfen, da DiskANN (Out-of-Core) für maximale Lese-Performance und Memory-Sharing zwingend auf direktes Memory-Mapping großer Vektor-Graphen angewiesen ist. Die Latenzeinbußen wären inakzeptabel.
*   **Begründung**: Mmap ist ein inhärent unsafer OS-Call, aber für High-Performance Vektor-Indizes unabdingbar. Die explizite Ausnahme legitimiert die Nutzung transparent und erzwingt gleichzeitig die Einhaltung lokaler `// SAFETY:`-Beweise, statt die generelle Code-Hygiene durch `#![allow(unsafe_code)]` auszuhebeln.

---
