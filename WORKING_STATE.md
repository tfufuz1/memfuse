# MemFuse — Working State
*Zuletzt aktualisiert: 2026-08-24 von Governance-Audit-Agent*

## Sprint-Status

| Sprint | Task | Status | Notizen |
|--------|------|--------|---------|
| 1 | fsync-Propagation (4× `let _ = sync_all()`) | ⏳ Offen | wal.rs:338,422,471 + lsm.rs:125 — AI-TAG[SMELL][CRITICAL] |
| 1 | SessionPool `.expect()` → Result | ⏳ Offen | memfuse-embed/src/lib.rs:43,45,51 — production panics |
| 1 | snapshot.rs `.expect()` → Result | ⏳ Offen | memfuse-core/src/snapshot.rs:207,258 |
| 1 | Atomic rename for DiskANN write_to_file | ⏳ Offen | Requires tmp+rename pattern |
| 2 | ADR-018 umsetzen: pip install README | ⏳ Offen | PyPI-Anleitung in README ergänzen |
| — | Governance-Overhaul (AGENTS.md v5) | ✅ Erledigt 2026-08-24 | Factual errors corrected, session protocol added |

## Offene AI-TAGs (automatisch prüfen!)

Stand letzter Prüfung: 2026-08-24
Befehl: `grep -rn "AI-TAG\[SMELL\]\[CRITICAL\]" crates/ --include="*.rs" | grep -v RESOLVED`
Ergebnis: **4 offene Tags** in memfuse-store:

| Datei | Zeile | ID | Problem |
|-------|-------|----|---------|
| `crates/memfuse-store/src/wal.rs` | 334 | AGT-AUDIT-006 | Silent `let _ = dir.sync_all()` |
| `crates/memfuse-store/src/wal.rs` | 418 | — | Silent `let _ = dir.sync_all()` |
| `crates/memfuse-store/src/wal.rs` | 467 | — | Silent `let _ = dir.sync_all()` |
| `crates/memfuse-store/src/lsm.rs` | 121 | — | Silent `let _ = dir.sync_all()` |

3 resolved Tags (CONVENTION-DRIFT, SPEC-DRIFT) in memfuse-db und memfuse-text.

## Offene .expect() in Produktionscode

| Datei | Zeilen | Kontext |
|-------|--------|---------|
| `memfuse-embed/src/lib.rs` | 43, 45, 51 | SessionPool pop/push — lock poisoning + exhaustion |
| `memfuse-core/src/snapshot.rs` | 207, 258 | proptest guarantee + guard index |

## Letzter ADR

Neuester ADR: ADR-018 (2026-08-24) — Doppelstrategie PyPI + Desktop-App (Auflösung ADR-007/ADR-009)

## Bekannte Konflikte / Blockaden

- **ADR-007 vs ADR-009**: Formal aufgelöst durch ADR-018 (2026-08-24). PyPI + Desktop als bewusste Doppelstrategie.
- **Zero-Panic Status**: ARCHITECTURE.md und SOT zeigten "🟢 Gehärtet" — korrigiert zu "🟡 In Arbeit" (2026-08-24).

## Nächste Agent-Session soll beginnen mit

- [ ] `WORKING_STATE.md` lesen
- [ ] `grep -rn "AI-TAG\[SMELL\]\[CRITICAL\]" crates/ --include="*.rs" | grep -v RESOLVED`
- [ ] `cargo test --workspace --exclude memfuse-tauri`
- [ ] Dann: **Sprint 1 — fsync-Propagation fixen** (4 Stellen in memfuse-store, `let _ =` → `?`)
