# ADR-018: Doppelstrategie — PyPI-Library UND Desktop-App (Auflösung ADR-007/ADR-009-Konflikt)


*   **Datum**: 2026-08-24
*   **Status**: ✅ Final
*   **Kontext**: ADR-007 (2026-07-19) erklärt PyPI als primären Vertriebskanal und verwirft Desktop-App. ADR-009 (2026-07-20, einen Tag später) beschloss den Aufbau von memfuse-tauri. Heute ist memfuse-tauri das größte Feature-Investment. Kein ADR hat ADR-007 formal revidiert — beide galten gleichzeitig als "final".
*   **Entscheidung**: MemFuse verfolgt eine bewusste Doppelstrategie:
    - **Kanal 1 — Desktop-App** (memfuse-tauri / "MemFuse Brain"): Zielgruppe DACH-Unternehmensanwender, nicht-technische Nutzer. Positionierung als lokaler, air-gapped Unternehmensassistent. Aktiv in Entwicklung, primäres UI-Investment.
    - **Kanal 2 — Library** (memfuse-py / memfuse-core): Zielgruppe Python-KI-Entwickler, Rust-Entwickler. Technisch fertig (maturin-Build, mcp-Dependencies), noch nicht in README dokumentiert. Nächster Schritt: `pip install`-Anleitung in README ergänzen.
*   **Alternativen**: Einer der beiden Kanäle wird aufgegeben. Verworfen — beide adressieren komplementäre Zielgruppen ohne Kannibalisierung.
*   **Begründung**: Die Desktop-App erreicht nicht-technische Nutzer über GUI-First-Erfahrung. Die Library erreicht KI-Entwickler über programmatische Integration. Beide teilen denselben Kern (memfuse-db, Layer 0–2). Die bisherige Inkohärenz lag nicht an der Strategie, sondern am fehlenden ADR der die Koexistenz formal legitimiert.
*   **Ersetzt**: ADR-007 bzgl. Vertriebskanal-Priorisierung (nicht bzgl. technischer Entscheidungen wie Zero-C-Deps, kein Docker).
*   **Ergänzt**: ADR-009 (Desktop-App-Grundstein).
*   **Konsequenzen**:
    - README-Aktualisierung (`pip install`-Anleitung) ist priorisierte Tech-Debt.
    - Bis dahin: memfuse-tauri als primäres User-facing Produkt behandeln.

---
