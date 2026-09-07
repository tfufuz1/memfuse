# ADR-028: Dezentrales Inline-Kontextsystem, Sekundengenaue Zeitstempel & Verpflichtendes Mehrfach-Session-Review


*   **Datum**: 2026-08-29
*   **Status**: ✅ Final
*   **Kontext**:
    1. `WORKING_STATE.md` war ein Merge-Konflikt-Hotspot, da jede Session Freitext und auto-generierte Blöcke in dieselben Zeilen derselben zentralen Datei schrieb. Bei hoher paralleler Jules-Sitzungsdichte führten konkurrierende PRs zu manueller Re-Intervention.
    2. Die Tages-Zeitstempel-Granularität (`TS:YYYY-MM-DD`) verhinderte die exakte Sequenzierung von Ereignissen innerhalb eines Tages bei bis zu 100 Sitzungen pro Tag.
    3. Sequenzielle IDs (`AGT-<CRATE>-NNN`) führten zu Zähler-Kollisionen bei parallelen Sitzungen.
    4. Es fehlte eine strukturierte Mehrfach-Session-Qualitätssicherung. Ein Einzel-Agent-Review leidet unter Bestätigungs-Bias.
*   **Entscheidung**:
    1. **`WORKING_STATE.md` als reine, voll-generierte Projektion**: Die Datei enthält NULL manuell editierten Freitext mehr und liegt vollständig in einem Auto-Marker-Block. Git-Merge-Konflikte in dieser Datei werden deterministisch durch erneutes Ausführen von `just sync-docs` aufgelöst.
    2. **Sekundengenaue Zeitstempel & Hash-IDs**: Alle neuen Tags tragen `TS:YYYY-MM-DDTHH:MM:SSZ` (UTC), ein Pflichtfeld `SESSION:<8-hex-hash>` und eine hash-basierte ID `AGT-<CRATE>-<8-hex-hash>` (`sha256(crate + pfad + zeile + ts)[..8]`). Bestehende `AGT-<CRATE>-NNN`-IDs bleiben unter Bestandsschutz.
    3. **Erweiterter `FILE-CONTEXT`-Kommunikationskanal**: Ergänzt um ein optionales `AGENT-NOTIZ:`-Feld als dezentraler Kommunikationskanal zwischen Sitzungen direkt am Code.
    4. **Verpflichtendes Mehrfach-Session-Review (`REVIEW-PASS`)**: Einführung der Grammatik `REVIEW-PASS[N/M] STATUS:PASS|FAIL|CONDITIONAL` mit Pflichtfeld `PRÜFER-KONTEXT: FRESH`. Jede `STATUS:DONE`-Markierung eines `ANCHOR` erfordert 2 (Standard) bzw. 3 (`ASK`/security/unsafe) `REVIEW-PASS`-Einträge mit unterschiedlichen `SESSION:`-Hashes.
    5. **CI Gate 8**: Unterbefehl `cargo xtask check-review-coverage` erzwingt die Mindestanzahl unabhängiger Review-Pässe in CI (`context-gates.yml`).
*   **Alternativen**:
    - Einbindung externer Go/Python Task-Management-Tools (z.B. Beads). Verworfen, um MemFuse sovereigntiesicher und ohne Netzwerk/neue Fremdabhängigkeiten nativ über Rust/`xtask` zu betreiben.
*   **Begründung**: Beseitigt Merge-Konflikte strukturell durch Konstruktion, stellt sekundengenaue Rückverfolgbarkeit her und eliminiert Bestätigungs-Bias bei Reviews durch das Unabhängigkeitsgebot.
*   **Konsequences**:
    - `rules/tag_taxonomy.md`, `rules/llm_protocol.md` (Schleife 8), `AGENTS.md §6` und `environment_script.sh` aktualisiert.
    - `xtask` generiert `WORKING_STATE.md` und `docs/CHANGELOG.md` deterministisch aus Inline-Tags.

---
