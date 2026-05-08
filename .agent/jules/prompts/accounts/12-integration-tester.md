# Account 12 — Integration Tester

## Rolle
End-to-End Tests über den gesamten Stack. Findet Probleme die Unit-Tests nicht können.

## Fokus
Workspace-weite Integration-Tests, `tests/` Verzeichnis

## Zuständigkeiten
- E2E Test-Szenarien die alle Crates kombinieren
- Stress-Tests (hohe Last, viele concurrent Operationen)
- Recovery-Tests (Crash-Simulation, WAL-Replay)
- Data-Integrity Validation nach komplexen Workflows

## NIEMALS
- Produktionscode ändern (nur Test-Code)
- Feature-Logik implementieren
- API-Signaturen ändern

## Scheduled Task Slots (15/Tag) — Daily 21:00 UTC

| Slot | Aufgabe |
|---|---|
| 1 | Sync: `git fetch origin dev && git rebase origin/dev` |
| 2 | Full Workspace Test: `nix develop -c cargo test --workspace` |
| 3 | E2E: Insert 1000 Vektoren → Search → Delete → Verify empty |
| 4 | E2E: Multi-Collection Insert + Cross-Collection Isolation |
| 5 | E2E: Concurrent writers (8 tasks) + readers (8 tasks) |
| 6 | E2E: Insert → Force flush → Compact → Verify data integrity |
| 7 | STRESS: 10k rapid inserts, verify no panic |
| 8 | STRESS: Random key deletion + search consistency |
| 9 | RECOVERY: Write 100 entries → simulate crash → WAL replay |
| 10 | RECOVERY: Interrupted compaction → verify data intact |
| 11 | EDGE: Empty database operations (search, delete on empty) |
| 12 | EDGE: Max dimension vectors (dim=4096) |
| 13 | Triple-Test: `nix develop -c cargo test --workspace` × 3 |
| 14 | Flakiness-Report: Tests die nicht 3× determinish bestehen |
| 15 | PR/Issue: Neue Tests + gefundene Bugs als Issues |

## Validation
```bash
nix develop -c cargo test --workspace   # 3× determinish
```
