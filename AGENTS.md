# MemFuse — Lead Agent Coordination Protocol

Welcome, Agent. You are operating on the **MemFuse Hybrid-Search Database**. This is your primary directive and coordination document.

---

## 🧭 PFLICHT-PRÄAMBEL (Mandatory Reading)
Bevor du Code schreibst oder änderst, MUSST du diese Dokumente gelesen haben:
1.  **Diese Datei** — Crate-Kontext und verbotene Muster.
2.  [CONSTITUTION.md](./CONSTITUTION.md) — Unveränderliche Prinzipien (Safety & Security).
3.  [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md) — Schichtmodell (DAG) und Invarianten.
4.  [docs/BACKLOG.md](./docs/BACKLOG.md) — Zentrale Task-Liste mit FIND-IDs.

---

## 🛡️ ZERO-PANIC ENFORCEMENT PROTOKOLL

-   **Kein `.unwrap()`, kein `.expect()`**: Nutze den `?`-Operator.
-   **Error Mapping**: Eigene Fehler müssen in `memfuse_core::MemFuseError` integriert werden.
-   **Safe Rust**: Alle Crates haben `#![forbid(unsafe_code)]`. Ausnahme: `memfuse-index` für SIMD (muss mit `// SAFETY:` dokumentiert sein).
-   **WAL-First**: Keine Datenänderung im Speicher, bevor das WAL synchronisiert wurde.

### Verbotene Muster
| ❌ Verboten | ✅ Korrekt |
|---|---|
| `.unwrap()` / `.expect()` | `?`-Operator mit `MemFuseError` |
| WAL-Write NACH MemTable-Write | Immer WAL zuerst, dann MemTable |
| HNSW Layer-Algorithmus ändern ohne Benchmark | Recall-Benchmark vorher + nachher |
| `tokio::spawn` ohne Cancellation-Handle | `handle.abort()` + Cleanup in Drop |
| Panicken in Agent Step Execution | `Err(MemFuseError::...)` + State-Sicherung |

---

## 🛠️ Operating Procedures (The Triple-Test-Gate)
Kein PR darf gemerdet werden, solange rote Tests existieren.
1.  `cargo check -p [CRATE]` (Lints & Warnings sind Fehler)
2.  `cargo test -p [CRATE]` (Unit & Integration)
3.  `just triple-test` (Wiederholte Stabilitätstests)

---

## 📦 Crate-Anweisungen & Fokus

| Crate | Lead Agent | FIND-IDs & Fokus |
|---|---|---|
| `memfuse-core` | @JULES-01 | **FIND-COR-001**: Trait-Bereinigung. Keine I/O. |
| `memfuse-store` | @JULES-02 | **FIND-STO-001**: WAL-CRC & Starvation. **FIND-STO-003**: Rollback. |
| `memfuse-index` | @JULES-03 | **FIND-IDX-001**: SIMD Safety. HNSW Persistence. |
| `memfuse-db` | @JULES-04 | **FIND-DB-001**: Snapshot Recovery. **FIND-DB-002**: Tracing. |
| `memfuse-text` | @JULES-05 | **FIND-TXT-001**: DAG-Fix. **FIND-TXT-002**: BM25 Stability. |
| `memfuse-crypto` | @JULES-06 | **FIND-CRY-001**: Salt-Fix. **FIND-CRY-002**: Nonce-Reuse. |
| `memfuse-graph` | @JULES-07 | **FIND-GRA-001**: Isolation & Traversal Performance. |
| `memfuse-saos` | @JULES-08 | **FIND-SAOS-001**: Atomic Final State. |
| `memfuse-sandbox`| @JULES-09 | **FIND-SBX-001**: Host-Funcs. **FIND-SBX-002**: AirGap. |
| `memfuse-py` | @JULES-10 | **FIND-PY-001**: Python Exception Mapping & MCP Tools. |

---

## 🚨 Aktuelle Mission
Priorität haben **TIER 1 (BLOCKING)** Tasks im [BACKLOG.md](./docs/BACKLOG.md).
Besonders kritisch: **FIND-CRY-002** (Nonce-Reuse) und **FIND-STO-003** (Rollback).
