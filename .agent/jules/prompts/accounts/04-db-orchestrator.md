# Account 04 — DB Orchestrator

## Rolle
Orchestrierung der Sub-Engines. API-Facade für Endnutzer.

## Fokus-Crate
`crates/memfuse-db/`

## Zuständigkeiten
- `MemFuse` Facade — Haupteinstiegspunkt
- `Collection` — Namespace-Isolation (WP-1.2)
- `HybridSearch` — RRF Orchestrierung (nach WP-2.1)
- Advanced Metadata Filtering (WP-4.2)

## Work Packages
| WP | Priorität | Dependency | Status |
|---|---|---|---|
| WP-1.2 | 🟠 HOCH | WP-1.1 DONE | Primary |
| WP-4.2 | 🟡 MITTEL | WP-1.2 DONE | Blocked |

## Backward-Compat-Guard
**ALLE bestehenden Contract-Tests müssen weiterhin grün sein.**
Die bestehende API (`MemFuse::insert/search/delete`) funktioniert unverändert über Default-Collection.

## NIEMALS
- LSM-Interna direkt ändern (→ Account 02)
- HNSW-Interna direkt ändern (→ Account 03)
- Bestehende API-Signaturen brechen

## Scheduled Task Slots (15/Tag) — Phase: WP-1.2

| Slot | Aufgabe |
|---|---|
| 1 | Sync: `git fetch origin dev && git rebase origin/dev` |
| 2 | Backward-Compat-Check: `nix develop -c cargo test -p memfuse-db` (alle bestehenden Tests) |
| 3 | SPEC lesen: `docs/specs/SPEC-*-WP-1.2-Collections.md` |
| 4 | RED: `test_collections_are_isolated` |
| 5 | RED: `test_drop_removes_all_data` |
| 6 | RED: `test_default_collection_compat` |
| 7 | RED: `test_list_collections` |
| 8 | GREEN: `Collection` struct + Key-Schema Implementierung |
| 9 | GREEN: `db.collection("name")` → Collection-Handle |
| 10 | GREEN: `list_collections()` + `drop_collection()` |
| 11 | GREEN: Default-Collection Backward-Compat Routing |
| 12 | REFACTOR: Doc-Comments für neue pub API |
| 13 | Triple-Test: `nix develop -c cargo test -p memfuse-db` × 3 |
| 14 | Clippy+Fmt + Workspace-Test: `nix develop -c cargo test --workspace` |
| 15 | PR: `feat(db): WP-1.2 Collections / Namespaces` |

## Key-Schema (aus SPEC)
```
Default Collection:  key as-is (Backward Compat)
Named Collection:    b"__col:{name}:\x00" + key.as_bytes()
Collection Index:    b"__col_idx:\x00" + name.as_bytes() → metadata
```

## Validation
```bash
nix develop -c cargo test -p memfuse-db   # 3× — ALLE Tests müssen grün sein (alte + neue)
nix develop -c cargo test --workspace     # Keine Regressionen
```
