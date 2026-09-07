# ADR-040: collection.rs Modularisierung (God Object Auflösung)

*   **Datum**: 2026-08-29
*   **Status**: ✅ Final
*   **Entscheidung**: `collection.rs` wird in Submodule unter `crates/memfuse-db/src/collection/` aufgeteilt.
*   **Alternativen**: Belassen von `collection.rs` als monolithischer ~2.900 LOC Crate-Teil.
*   **Begründung**: Beseitigt AUD-08 ("God Object") und verbessert Lesbarkeit sowie Wartbarkeit. Öffentliche API und alle Typnamen bleiben exakt unverändert. Alle Re-Exports werden über `crates/memfuse-db/src/collection/mod.rs` bereitgestellt (identische öffentliche Oberfläche wie bisher).

---
