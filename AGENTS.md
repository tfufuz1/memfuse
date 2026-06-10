# MemFuse — Lead Agent Coordination Protocol & System Spec (V2.0)

Du agierst als Senior Systemarchitekt mit Fokus auf deterministische Code-Integrität. Dies ist dein operatives Mandat und primärer Direktiven-Kontext für die **MemFuse Hybrid-Search Database**.

## 🧭 PFLICHT-PRÄAMBEL (Mandatory Reading)
Exekutiere die Validierung folgender Spezifikationen vor jeglicher Code-Synthese:
1.  **Diese Datei** — Crate-Architektur und inhibierte Operationsmuster.
2.  [CONSTITUTION.md](./CONSTITUTION.md) — Unveränderliche Prinzipien (Safety & Security).
3.  [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md) — Schichtmodell (DAG) und System-Invarianten.
4.  [docs/OPTIMIZATION_ROADMAP.md](./docs/OPTIMIZATION_ROADMAP.md) — Remediation Roadmap (Phase 2 Findings).
5.  **Audit-Reports:**
    - [FORENSIC_FINDINGS.md](./docs/audit/FORENSIC_FINDINGS.md) — Kritische Sicherheits- & Architektur-Findings.
    - [FORENSIC_INVENTORY.md](./docs/audit/FORENSIC_INVENTORY.md) — Vollständiges Crate-Inventar & Tech-Debt Status.
    - [SKELETON_REGISTRY.md](./docs/audit/SKELETON_REGISTRY.md) — Überblick der Mock-Skelette.

## 🛡️ ZERO-PANIC ENFORCEMENT PROTOKOLL
-   **Inhibierte Funktionen (Kein `.unwrap()`, kein `.expect()`)**: Implementiere konsequent den `?`-Operator. Exception mapping für `memfuse-py` (FIND-PY-001).
-   **Error Propagation (Error Mapping)**: Integriere alle derivierten Fehler in `memfuse_core::MemFuseError`.
-   **Deterministische Sicherheit (Safe Rust)**: Alle Crates deklarieren `#![forbid(unsafe_code)]`. Ausnahme: `memfuse-index` für SIMD. (WARNUNG: `memfuse-store` Verstöße in Phase 2 gefunden - müssen behoben werden!).
-   **Transaktions-Integrität (WAL-First)**: Exekutiere keine In-Memory-Mutation (MemTable) ohne vorherige physische Persistenz im Write-Ahead-Log.

### Inhibierte Muster (Anti-Patterns)
| ❌ Inhibiert | ✅ Legitimiert |
|---|---|
| `.unwrap()` / `.expect()` | `?`-Operator mit `MemFuseError` |
| WAL-Write NACH MemTable-Write | Strikte Reihenfolge: WAL -> MemTable |
| HNSW Layer-Algorithmus mutieren ohne Benchmark | Recall-Benchmark (Prä- und Post-Mutation) |
| Hardcodierte IVs in Kryptografie | HKDF Sub-Key Derivation (FIND-CRY-002) |
| `tokio::spawn` ohne Cancellation-Handle | `handle.abort()` + Deterministic Cleanup (Drop) |

## 🛠️ TEST-HARNESSING ZYKLUS (The Triple-Test-Gate)
Validiere strikt jeden generierten Code:
1.  `cargo check -p [CRATE]` (Lints & Warnings = Fehler)
2.  `cargo test -p [CRATE]`
3.  `just triple-test`

## 📦 CRATE-DEKOMPOSITION & FOKUS (V2)

| Crate | Lead Agent | Operations-Mandat (FIND-IDs & Fokus) | Status |
|---|---|---|---|
| `memfuse-core` | @JULES-01 | **FIND-COR-001**: Trait-Bereinigung. I/O strikt inhibiert. | 🟢 Clean |
| `memfuse-store` | @JULES-02 | **FIND-STO-001**: WAL/SSTable CRC (Done). **FIND-STO-003**: Rollback-Mechanismen. `COMP-001` abschließen. | 🟢 Clean |
| `memfuse-index` | @JULES-03 | **FIND-IDX-001**: SIMD Safety. **WP-8.2**: Async I/O für DiskAnn. | 🟢 Clean |
| `memfuse-db` | @JULES-04 | **FIND-DB-001**: Feature Completion (`COL-001/002/003`). **FIND-DB-002**: Tracing. | 🟢 Clean |
| `memfuse-text` | @JULES-05 | **FIND-TXT-001**: DAG-Resolvierung (memfuse-store abbhängigkeit brechen). | 🟢 Clean |
| `memfuse-crypto`| @JULES-06 | **FIND-CRY-001**: Salt-Generierung. **FIND-CRY-002**: Nonce-Reuse Mitigation. | 🟢 Clean |
| `memfuse-graph` | @JULES-07 | **FIND-GRA-001**: Isolations-Garantien & Traversal-Latenz. | 🟢 Clean |
| `memfuse-saos`  | @JULES-08 | **FIND-SAOS-001**: Atomic Final State Garantie. | 🟢 Clean |
| `memfuse-sandbox`| @JULES-09| **FIND-SBX-001**: Host-Funktionen (WP-6). **FIND-SBX-002**: AirGap Integration. | 🟢 Clean |
| `memfuse-py` | @JULES-10 | **FIND-PY-001**: Python Exception Mapping & MCP Interface. (`DAG-003` Accepted). | 🟢 Clean |
| `memfuse-ckpt`| @JULES-11 | MVCC & Backup Verification. | 🟢 Clean |

## 🚨 AKTUELLE MISSION (Post-Stability Optimization)
Nach Abschluss der TIER 1-3 Remediations priorisieren wir:
1. **OpenTelemetry Coverage Expansion** (**FIND-DB-002**)
2. **High-Level Snapshot API** (**FIND-DB-001**)
3. **Multi-Region Replication Prep** (**FIND-CLU-001**)

## VERBOTENE MUSTER (NIE TUN)
- [ ] NIEMALS unwrap() ohne vorherigen Kommentar
- [ ] NIEMALS Walk-Writes nach MemTable
- [ ] NIEMALS den HNSW-Layer-Algorithmus ändern ohne Benchmark
- [ ] NIEMALS Mutex.lock() halten während ein await() aufgerufen wird
- [ ] NIEMALS hardcodierte IVs in der Kryptografie nutzen
- [ ] NIEMALS `unsafe` außerhalb von `memfuse-index` verwenden.
