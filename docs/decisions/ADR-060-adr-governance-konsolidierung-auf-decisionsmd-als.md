# ADR-060: ADR-Governance — Konsolidierung auf DECISIONS.md als Einzel-Quelle

*   **Datum**: 2026-09-04
*   **Status**: ✅ Final
*   **Entscheidung**: `docs/decisions/` wird aufgelöst. `DECISIONS.md` im Root-Verzeichnis ist die einzige kanonische Quelle für Architecture Decision Records (ADRs).
*   **Begründung**: Einhaltung des MECE-Prinzips aus `CONSTITUTION.md` ("Jede Information lebt an genau EINEM Ort"). Das Dual-System (`DECISIONS.md` vs. `docs/decisions/`) erzeugte Nummernkollisionen und Verwirrung. Das xtask-Tooling kennt und prüft primär `DECISIONS.md`.

---
