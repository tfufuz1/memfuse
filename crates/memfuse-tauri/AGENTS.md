# AGENTS.md — memfuse-tauri
> Ergänzung zu Root-AGENTS.md. Nur crate-spezifische Regeln hier.

## Kritische Invarianten dieser Crate
- `SystemTime` als TxId ist verboten (Race-Condition bei EMBED_CONCURRENCY>1) → `collection.allocate_tx()` verwenden.
- Keine Trait-Duplikate: prüfe vor jedem neuen Trait ob er in `memfuse-core` existiert.
- Scope `parking_lot::RwLockReadGuard` drops vor `.await` Points in IPC-Commands.

## Bekannte Fallstricke
- Document Ingestion Logik verbleibt in `ingestion/` zur Isolation schwerer Parser-Deps.

## Relevante rules/*.md
- `rules/async-io.md` — Non-blocking IPC & UI Responsiveness

## Pflicht-Tests (ANCHOR-Status)
- ANCHOR[TEST:TAU-001] STATUS:DONE — Ingestion Pipeline End-to-End Test (in `tests/e2e_test.rs`)
