# ADR-007: Produktstrategie — Lokale Agent-Memory-Library (Richtung C) [TEILWEISE ERSETZT durch ADR-018 bzgl. Vertriebskanal-Priorisierung, 2026-08-24]

*   **Datum**: 2026-07-19
*   **Status**: ✅ Final
*   **Entscheidung**: MemFuse wird als **eingebettete 4-Signal-Memory-Engine für lokale AI-Agenten** positioniert — kein Server, kein Docker, kein Cloud-Account. Primäre Vertriebskanäle: `pip install memfuse` (PyPI) und `cargo add memfuse-db` (crates.io). Richtung A (Sovereign Edge-DB) ist der langfristige Erweiterungspfad auf derselben Codebasis, nicht ein separater Pivot.
*   **Alternativen**:
    - (A) Air-Gapped / Sovereign Edge-DB — strategisch wertvoll, aber Enterprise-Vertrieb als Solo-Entwickler aktuell nicht realisierbar.
    - (B) DACH Enterprise-Search (Morphologie-Fokus) — das Morphologie-Merkmal ist zu schmal für ein eigenständiges Produkt, aber wertvoll als Differenzierungsfeature innerhalb von C.
*   **Begründung**: Option C erfordert den geringsten Pivot (80% des Codes existiert bereits), liefert in 4–8 Wochen überprüfbares Feedback (Benchmarks, PyPI-Downloads statt 12+ Monate Enterprise-Verkaufszyklen), und schließt Richtung A nicht aus — im Gegenteil: Zero-C-Deps und ACID-Garantien sind der Vorbereitungsschritt für Sovereign Edge. Die Sovereign-Core-Eigenschaften bleiben vollständig erhalten.
*   **Konsequenzen**:
    - `memfuse-graph` und `memfuse-py` werden in den aktiven Workspace reaktiviert (höchste Priorität).
    - `memfuse-cluster`, `memfuse-sandbox`, `memfuse-saos-agent` wurden physisch aus dem Repo entfernt (ausgelagert).
    - README und alle Governance-Dokumente werden auf "eingebettete Agent-Memory-Library" ausgerichtet.

---
