<context>
# MemFuse — Lead Agent Coordination Protocol

Du agierst als Senior Systemarchitekt mit Fokus auf deterministische Code-Integrität. Dies ist dein operatives Mandat und primärer Direktiven-Kontext für die **MemFuse Hybrid-Search Database**.
</context>

<instructions>
## 🧭 PFLICHT-PRÄAMBEL (Mandatory Reading)
Exekutiere die Validierung folgender Spezifikationen vor jeglicher Code-Synthese:
1.  **Diese Datei** — Crate-Architektur und inhibierte Operationsmuster.
2.  [CONSTITUTION.md](./CONSTITUTION.md) — Unveränderliche Prinzipien (Safety & Security).
3.  [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md) — Schichtmodell (DAG) und System-Invarianten.
4.  [docs/BACKLOG.md](./docs/BACKLOG.md) — Zentrale Operations-Spezifikation (FIND-IDs).
</instructions>

<policy>
## 🛡️ ZERO-PANIC ENFORCEMENT PROTOKOLL

-   **Inhibierte Funktionen (Kein `.unwrap()`, kein `.expect()`)**: Implementiere konsequent den `?`-Operator.
-   **Error Propagation (Error Mapping)**: Integriere alle derivierten Fehler in `memfuse_core::MemFuseError`.
-   **Deterministische Sicherheit (Safe Rust)**: Alle Crates deklarieren `#![forbid(unsafe_code)]`. Ausnahme: `memfuse-index` für SIMD (erfordert explizite `// SAFETY:` Deklaration).
-   **Transaktions-Integrität (WAL-First)**: Exekutiere keine In-Memory-Mutation (MemTable) ohne vorherige physische Persistenz im Write-Ahead-Log.

### Inhibierte Muster (Anti-Patterns)
| ❌ Inhibiert | ✅ Legitimiert |
|---|---|
| `.unwrap()` / `.expect()` | `?`-Operator mit `MemFuseError` |
| WAL-Write NACH MemTable-Write | Strikte Reihenfolge: WAL -> MemTable |
| HNSW Layer-Algorithmus mutieren ohne Benchmark | Recall-Benchmark (Prä- und Post-Mutation) |
| `tokio::spawn` ohne Cancellation-Handle | `handle.abort()` + Deterministic Cleanup (Drop) |
| Panicken in Agent Step Execution | `Err(MemFuseError::...)` + State-Sicherung |
</policy>

<test_harness>
## 🛠️ TEST-HARNESSING ZYKLUS (The Triple-Test-Gate)
Validiere strikt jeden generierten Code durch den iterativen Test-Harnessing-Zyklus (**Generieren -> Testen -> Scheitern -> Reflektieren -> Korrigieren**).
Keine Code-Integration ohne fehlerfreie Metriken. Exekutiere diese Sequenz zur Qualitätskontrolle:
1.  `cargo check -p [CRATE]` (Lints & Warnings resultieren in unmittelbaren Fehlerzuständen)
2.  `cargo test -p [CRATE]` (Vollständige Unit- & Integrations-Validierung)
3.  `just triple-test` (Wiederholte Stabilitäts-Evaluation)
</test_harness>

<assignments>
## 📦 CRATE-DEKOMPOSITION & FOKUS

| Crate | Lead Agent | Operations-Mandat (FIND-IDs & Fokus) |
|---|---|---|
| `memfuse-core` | @JULES-01 | **FIND-COR-001**: Trait-Bereinigung. I/O strikt inhibiert. |
| `memfuse-store` | @JULES-02 | **FIND-STO-001**: WAL-CRC & Starvation. **FIND-STO-003**: Rollback-Mechanismen. |
| `memfuse-index` | @JULES-03 | **FIND-IDX-001**: SIMD Safety. HNSW Persistenz-Modelle. |
| `memfuse-db` | @JULES-04 | **FIND-DB-001**: Snapshot Recovery. **FIND-DB-002**: Tracing-Architektur. |
| `memfuse-text` | @JULES-05 | **FIND-TXT-001**: DAG-Resolvierung. **FIND-TXT-002**: BM25 Stabilität. |
| `memfuse-crypto` | @JULES-06 | **FIND-CRY-001**: Salt-Generierung. **FIND-CRY-002**: Nonce-Reuse Mitigation. |
| `memfuse-graph` | @JULES-07 | **FIND-GRA-001**: Isolations-Garantien & Traversal-Latenz. |
| `memfuse-saos` | @JULES-08 | **FIND-SAOS-001**: Atomic Final State Garantie. |
| `memfuse-sandbox`| @JULES-09 | **FIND-SBX-001**: Host-Funktionen. **FIND-SBX-002**: AirGap Integration. |
| `memfuse-py` | @JULES-10 | **FIND-PY-001**: Python Exception Mapping & MCP Interface. |
</assignments>

<mission>
## 🚨 AKTUELLE MISSION (Spec-Driven Focus)
Priorisiere **TIER 1 (BLOCKING)** Spezifikationen im [BACKLOG.md](./docs/BACKLOG.md).
Maximale Kritikalität erfordert Fokussierung auf: **FIND-CRY-002** (Nonce-Reuse Mitigation) und **FIND-STO-003** (Rollback-Integrität).
</mission>

## VERBOTENE MUSTER (NIE TUN)
- [ ] NIEMALS unwrap() ohne vorherigen Kommentar warum es sicher ist
- [ ] NIEMALS einen WAL-Write nach einem MemTable-Write (Reihenfolge KRITISCH)
- [ ] NIEMALS den HNSW-Layer-Assignment-Algorithmus ändern ohne Recall-Benchmark
- [ ] NIEMALS Mutex.lock() halten während ein await() aufgerufen wird
- [ ] NIEMALS hardcodierte IVs in der Kryptografie nutzen
