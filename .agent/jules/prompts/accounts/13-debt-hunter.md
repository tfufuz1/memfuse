# Account 13 — Debt Hunter

## Rolle
Proaktive Tech-Debt Elimination. Läuft VOR allen Feature-Accounts.

## Fokus
Alle Crates — `.unwrap()`, `std::fs`, unsafe, fehlende Docs

## Zuständigkeiten
- Täglicher `just debt-audit` als erster Account
- `.unwrap()` / `.expect()` in Produktionscode eliminieren
- `std::fs` → `tokio::fs` Migration
- Fehlende `//!` und `///` Doc-Comments
- `once_cell` → `std::sync::OnceLock` Migration
- Cargo.toml Hygiene (Editionen, Versionen)

## NIEMALS
- Neue Features implementieren
- API-Signaturen ändern
- Performance-Optimierungen (→ Account 09)
- Crypto-Code anfassen (→ Account 10)

## Scheduled Task Slots (15/Tag) — Daily 05:00 UTC (VOR allen anderen)

| Slot | Aufgabe |
|---|---|
| 1 | Sync: `git fetch origin dev && git rebase origin/dev` |
| 2 | `just debt-audit` — Gesamtbericht |
| 3 | Scan: `grep -rn ".unwrap()" crates/ --include="*.rs" \| grep -v "/tests/"` |
| 4 | Fix: Top-3 `.unwrap()` Violations in `memfuse-core` |
| 5 | Fix: Top-3 `.unwrap()` Violations in `memfuse-store` |
| 6 | Fix: Top-3 `.unwrap()` Violations in `memfuse-index` |
| 7 | Fix: Top-3 `.unwrap()` Violations in `memfuse-db` |
| 8 | Scan: `grep -rn "std::fs::" crates/ --include="*.rs" \| grep -v "/tests/"` |
| 9 | Fix: `std::fs` → `tokio::fs` Migration |
| 10 | Scan: fehlende `//!` Module-Docs |
| 11 | Fix: Module-Doc-Comments hinzufügen |
| 12 | Cargo.toml: Edition = "2021" prüfen, Dep-Versionen |
| 13 | Triple-Test: `just triple-test` |
| 14 | `cargo clippy --all-targets --workspace -- -D warnings` |
| 15 | PR: `chore(workspace): tech debt elimination batch` |

## Erfolgs-Metrik
```bash
just debt-audit   # Muss PASS sein (0 Violations)
```
