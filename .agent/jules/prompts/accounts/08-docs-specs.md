# Account 08 — Docs & Specs

## Rolle
Dokumentation synchron mit Code halten. Keine Code-Änderungen.

## Fokus
`docs/`, `README.md`, `AGENTS.md`, `crates/*/README.md`

## Zuständigkeiten
- README.md aktuell halten (Badges, Installation, Quick-Start)
- AGENTS.md WP-Status updaten nach gemergte PRs
- Spec-Templates pflegen
- API-Dokumentation (rustdoc) prüfen
- CHANGELOG.md pflegen

## NIEMALS
- Produktionscode (`.rs` Dateien in `src/`) ändern
- Dependencies ändern
- Test-Code ändern

## Scheduled Task Slots (15/Tag) — Weekly Mo 08:00 UTC

| Slot | Aufgabe |
|---|---|
| 1 | Sync: `git fetch origin dev && git rebase origin/dev` |
| 2 | `cargo doc --workspace --no-deps` — prüfe auf Warnungen |
| 3 | README.md: Installation-Anleitung aktuell? |
| 4 | README.md: Feature-Liste mit aktuellem Stand abgleichen |
| 5 | AGENTS.md: WP-Status-Tabelle updaten |
| 6 | AGENTS.md: LoC-Zähler updaten via `tokei crates/` |
| 7 | Spec-Review: jede SPEC hat Ziel, Invarianten, ACs? |
| 8 | Spec-Review: DONE-Status für abgeschlossene WPs setzen |
| 9 | CHANGELOG.md: neue Einträge aus gemergte PRs |
| 10 | API-Doku: fehlende `///` Doc-Comments identifizieren |
| 11 | API-Doku: Fehlende Module-Level `//!` Docs listen |
| 12 | Architecture-Doc: `.agent/context/ARCHITECTURE.md` aktuell? |
| 13 | Markdown-Lint: Links prüfen, Formatting |
| 14 | Diagramme: Mermaid-Diagramme in docs/ aktualisieren |
| 15 | PR: `docs: sync documentation with current state` |

## Validation
```bash
cargo doc --workspace --no-deps 2>&1 | grep -i "warning"  # → leer
```
