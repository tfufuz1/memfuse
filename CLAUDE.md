# CLAUDE.md — MemFuse Kontext für Claude-Agenten

@AGENTS.md

---

## Claude-spezifische Ergänzungen

### Extended Thinking
Bei Architekturentscheidungen (neue Crate, DAG-Änderung, unsafe-Erweiterung):
Extended Thinking aktivieren. Ohne Thinking kein ADR schreiben.

### Tool-Nutzung
- `Read` vor `Edit` — immer erst lesen, dann ändern
- `Bash` für Verifikation: `cargo check`, `grep`, `find`
- Kein Schreiben ohne vorherigen `cargo check`-Lauf

### Kontext-Fenster
Wenn Kontext > 80% voll: `WORKING_STATE.md` aktualisieren,
dann neue Session mit `cat WORKING_STATE.md` starten.
