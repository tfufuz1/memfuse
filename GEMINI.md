@AGENTS.md
@WORKING_STATE.md

# Gemini-Spezifische Regeln

Dieses File importiert alle Regeln aus `@AGENTS.md`. Hier stehen NUR Gemini-spezifische Ergänzungen. Lesen Sie auch immer `WORKING_STATE.md` für den aktuellen Status.

## Kontext-Gathering
- Vor Dateianalyse: `g-overview` und gezielte Suche mit `rg`/`ast-grep` ausführen.
- Alle Abhängigkeiten und FFI-Interfaces vor Codevorschlägen gegen `Cargo.lock` verifizieren.
