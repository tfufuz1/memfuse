# MemFuse — Audit Intake Verification Protocol (`AUDIT_INTAKE_PROTOCOL.md`)

> **Regel (AGENTS.md §4)**: Jeder Befund ("Finding") aus einem extern zugelieferten Audit-Dokument, Prompt oder Review-Bericht MUSS vor der Implementierung am AKTUELLEN Quellcode verifiziert werden.

---

## 📋 Verifikations-Ablauf (Schritt-für-Schritt)

1. **Datei & Zeile öffnen**:
   - Die im Finding genannte Datei an der angegebenen Zeilennummer im AKTUELLEN Quellcode öffnen.
   - Nicht auf historische Prompts oder alte Audit-Berichte verlassen — der Code könnte zwischenzeitlich refactored worden sein.

2. **Invariante & Zustand prüfen**:
   - Existiert das gemeldete Problem tatsächlich noch im aktuellen Stand?
   - Liegt die genannte Stelle vielleicht in Test-Code (`#[cfg(test)]` / `tests/`), der von den Produktionsregeln ausgenommen ist (z.B. `.unwrap()` in Unit-Tests)?
   - Wurde das Problem bereits durch einen früheren PR oder Refactoring behoben?

3. **Kategorisierung & Dokumentation im PR**:
   - **Falls AKTIV**: Problem beheben, mit passendem `AI-TAG` versehen und Test/Assertion hinzufügen.
   - **Falls ENTKRÄFTET / OBSOLET**:
     - Den Finding im PR-Kommentar oder Session-Log explizit als **`[ENTKRÄFTET]`** markieren.
     - Begründung beifügen (z.B. *"Zeile 142 liegt in #[cfg(test)] Modul"*, *"Refactored in Commit abc1234 — Typ existiert nicht mehr"*).
     - **NIEMALS** blind fiktiven Code schreiben oder unzutreffende Findings stillschweigend abarbeiten.

---

## 🚫 Anti-Patterns (Verboten)

- ❌ **Blind-Implementierung**: Codeänderungen vornehmen, ohne die betreffende Datei vorher geöffnet zu haben.
- ❌ **Stilles Ignorieren**: Einen unzutreffenden Finding einfach wegzulassen, ohne im PR zu erklären, warum er unzutreffend war.
- ❌ **Copy-Paste von Stale-Audit-Prompts**: Veraltete Audit-Texte unbereinigt in neue Aufgaben-Prompts übernehmen.
