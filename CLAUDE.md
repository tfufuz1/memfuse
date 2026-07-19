@AGENTS.md

# Claude-Spezifische Regeln

Dieses File importiert alle Regeln aus `@AGENTS.md`. Hier stehen NUR Claude-spezifische Ergänzungen.

## Plan-Mode-Verhalten
- Vor nicht-trivialen Änderungen: ADR in `DECISIONS.md` entwerfen und auf Freigabe warten (§3 Ask-First).
- Aktiven Backlog und Crate-Status aus `docs/SOURCE_OF_TRUTH.md` lesen, bevor Code generiert wird.
- Rollentrennung (§6 Schleife 7): Planer- und Implementierer-Phase strikt trennen.

## Erlaubte Befehle
- Verifikationsbefehle aus §0 dürfen ohne Rückfrage ausgeführt werden.
