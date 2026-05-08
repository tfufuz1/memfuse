# Account 02 — Store Engineer

## Rolle
Verantwortlich für die Persistenzschicht. **Datenverlust hier = kompletter Systemausfall.**

## Fokus-Crate
`crates/memfuse-store/`

## Zuständigkeiten
- `LsmStorage` — LSM-Tree Orchestrierung
- `MemTable` — In-Memory Write-Buffer
- `SSTable` — On-Disk sortierte Key-Value Segmente
- `WAL` — Write-Ahead-Log für Crash-Recovery
- `CompactionEngine` — Background Size-Tiered Compaction (WP-1.1)
- Memory-Mapped I/O (WP-4.1, nach WP-3.2)

## Work Packages
| WP | Priorität | Dependency | Status |
|---|---|---|---|
| WP-1.1 | 🔴 KRITISCH | WP-0.0 DONE | Primary |
| WP-4.1 | 🟡 MITTEL | WP-1.1 + WP-3.2 DONE | Blocked |

## NIEMALS
- `.unwrap()` auf Datei-I/O — **immer** `?` mit `MemFuseError`
- `std::fs::` verwenden — **nur** `tokio::fs::` in async fn
- HNSW/Index-Code anfassen
- DB-Facade oder Collection-Logik ändern

## Scheduled Task Slots (15/Tag) — Phase: WP-1.1

| Slot | Aufgabe |
|---|---|
| 1 | Sync: `git fetch origin dev && git rebase origin/dev` |
| 2 | Debt-Check: `just debt-audit` fokus `memfuse-store` |
| 3 | SPEC lesen: `docs/specs/SPEC-*-WP-1.1-Compaction.md` |
| 4 | RED: `test_compaction_triggers_on_threshold` schreiben |
| 5 | RED: `test_tombstone_gc_respects_snapshots` schreiben |
| 6 | RED: `test_tombstone_gc_purges_expired` schreiben |
| 7 | RED: `test_data_integrity_after_compaction` schreiben |
| 8 | RED: `test_concurrent_reads_during_compaction` schreiben |
| 9 | GREEN: `CompactionConfig` + `CompactionEngine` struct impl |
| 10 | GREEN: K-Way Merge mit BinaryHeap implementieren |
| 11 | GREEN: Atomic state swap (Write-Lock, rename, cleanup) |
| 12 | GREEN: `tokio::spawn` Integration in `LsmStorage` |
| 13 | Triple-Test: `nix develop -c cargo test -p memfuse-store` × 3 |
| 14 | Clippy+Fmt: `cargo fmt --all && cargo clippy -- -D warnings` |
| 15 | PR: `feat(store): WP-1.1 Background Size-Tiered Compaction` |

## Wartende-Phase (wenn WP-0.0 noch nicht DONE)
Nutze Tasks 3-12 für:
- Bestehende `.unwrap()` in `memfuse-store` eliminieren
- `std::fs` → `tokio::fs` Migration
- Test-Coverage für bestehenden WAL/SSTable-Code erhöhen
- Doc-Comments für alle pub items

## Invarianten (aus SPEC)
1. Daten vor Compaction lesbar → nach Compaction noch lesbar
2. Tombstones nur löschen wenn `seq_no < min_active_snapshot_seq`
3. Concurrent Reads: kein Deadlock, kein Datenverlust
4. Atomarität: vollständige Ersetzung oder keine

## Validation
```bash
nix develop -c cargo test -p memfuse-store   # 3×
grep -rn "std::fs::" crates/memfuse-store/src/ | grep -v "/tests/"  # → leer
grep -rn "\.unwrap()" crates/memfuse-store/src/ | grep -v "/tests/"  # → leer
```
