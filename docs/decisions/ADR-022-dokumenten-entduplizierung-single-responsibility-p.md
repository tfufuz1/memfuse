# ADR-022: Dokumenten-Entduplizierung & Single Responsibility Protocol


*   **Datum**: 2026-08-27
*   **Status**: ✅ Final
*   **Kontext**: Bisher trugen `AGENTS.md`, `docs/SOURCE_OF_TRUTH.md`, `docs/ARCHITECTURE.md` und `WORKING_STATE.md` teilweise identische Fakten (Crate-Listen, Layer-DAG, Sprint-Historien) redundant und manuell gepflegt vor. Dies führte zu Drift-Risiken.
*   **Entscheidung**:
    - Strikte Trennung der Dokumentenzuständigkeiten gemäß "Dokumenten-Landkarte":
      - `AGENTS.md`: Verbindliche Verhaltensregeln (manuell, stabil).
      - `docs/ARCHITECTURE.md`: Technische Ist-Architektur (DAG, Layer, Crate-Zweck — **auto-generiert** via `xtask sync-docs`).
      - `docs/SOURCE_OF_TRUTH.md`: Produktstrategie, Roadmap, Entscheidungskontext (WARUM — manuell + auto-generierte Crate-Inventartabelle).
      - `WORKING_STATE.md`: Nur Session-zu-Session-Handoff (aktueller Zustand, offene Tags — auto-generiert + minimaler manueller Zusatz).
      - `docs/CHANGELOG.md`: Historische Sprint-Tabelle (aus `WORKING_STATE.md` ausgelagert).
      - `DECISIONS.md`: Chronologisches ADR-Log (manuell).
    - Konsistenzprüfung `cargo run -p xtask -- check-consistency` schlägt fehl, wenn manuell genannte Zahlen (z. B. Crate-Anzahl in `AGENTS.md` oder `README.md`) von der tatsächlichen `Cargo.toml`-Workspace-Topologie abweichen.
*   **Alternativen**: Weiterhin manuelle Redundanzen in mehreren Dateien pflegen. Verworfen wegen hohem Wartungsaufwand und Inkonsistenzgefahr.
*   **Begründung**: Single Responsibility Prinzip für Dokumentation stellt sicher, dass Fakten nur an genau einem Ort gepflegt oder automatisch generiert werden.
*   **Konsequenzen**:
    - `xtask` wird um `check-consistency` und CLI-Flag `--check` für `sync-docs` erweitert.
    - Gate 8 in `context-gates.yml` schützt gegen manuelle Inhaltsabweichungen und Drift.

---
