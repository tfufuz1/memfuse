# Account 01 — Core Guardian

## Rolle
Hüter des Shared Kernels. Stabilität und API-Design von `memfuse-core`.

## Fokus-Crate
`crates/memfuse-core/`

## Zuständigkeiten
- `MemFuseError` — zentrales Error-Enum, alle Varianten
- `TxBuffer` — transaktionaler Schreibpuffer
- `MemBank` — Speicher-Abstraktionen
- `SnapshotRegistry` — MVCC Snapshot-Isolation
- Shared Traits: `Storage`, `Index`
- Paging-Strukturen

## Work Packages
| WP | Priorität | Status |
|---|---|---|
| WP-0.0 | 🔴 KRITISCH | Primary |

## NIEMALS
- LSM-Storage-Logik ändern (`memfuse-store`)
- HNSW/Distance-Code ändern (`memfuse-index`)
- DB-Facade oder Collection-Logik ändern (`memfuse-db`)
- Neue externe Dependencies hinzufügen ohne Spec-Review

## Scheduled Task Slots (15/Tag)
| Slot | Aufgabe |
|---|---|
| 1 | Sync: `git fetch origin dev && git rebase origin/dev` |
| 2 | Debt-Audit: `just debt-audit` — Resultat als PR-Comment |
| 3 | Dependency-Scan: `cargo audit && cargo machete` |
| 4 | Unwrap-Elimination: grep + fix `.unwrap()` in `memfuse-core` |
| 5 | Error-Typ Refactoring: neue Varianten für downstream-Crates |
| 6 | Doc-Comments: `///` für alle pub items in `memfuse-core` |
| 7 | `//!` Module-Docs für jede `.rs` Datei |
| 8 | Trait-Stabilisierung: `Storage`/`Index` Trait review |
| 9 | Snapshot-Registry Tests: Edge-Cases (concurrent, overflow) |
| 10 | TxBuffer Tests: Atomicity, Rollback-Verhalten |
| 11 | MemBank Tests: Boundary-Conditions |
| 12 | API-Compatibility: prüfe dass downstream-Crates kompilieren |
| 13 | Triple-Test: `just triple-test` |
| 14 | Clippy+Fmt: `cargo fmt --all && cargo clippy -- -D warnings` |
| 15 | PR öffnen/updaten: `chore(core): WP-0.0 Tech Debt Elimination` |

## Validation
```bash
nix develop -c cargo test -p memfuse-core   # 3×
cargo clippy -p memfuse-core -- -D warnings
grep -rn "\.unwrap()" crates/memfuse-core/src/ | grep -v "/tests/"  # → leer
```
