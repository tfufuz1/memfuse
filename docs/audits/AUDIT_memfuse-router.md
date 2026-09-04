# AUDIT REPORT: `memfuse-router`

**Datum:** 2026-09-02 (Aktualisiert: 2026-09-04)
**Auditor:** Senior Rust Routing-Engineer
**Crate:** `crates/memfuse-router` · Layer 3 (SLM-Routing Engine)
**Ziel-Repository:** MemFuse (`https://github.com/tfufuz1/memfuse`)

---

## 1. Executive Summary

Das Crate `memfuse-router` stellt die SLM (Small Language Model)-Routing Engine in Layer 3 der MemFuse-Architektur bereit. Gemäß **ADR-020 (MemFuse Brain)** und dem **Layered Architecture Constraint (DAG 0->4)** verarbeitet die Router Engine hybride Suchergebnisse von `memfuse-db` (Layer 2) und bestimmt deterministisch das optimale SLM-Profil für die Anfrage basierend auf Graph-Community-Zuordnungen, Relevanzwerten und konfigurierte Token-Budgets.

### Kernaussagen des Audits:
1. **Branch & Line Coverage:** **PASSED (99.18% line coverage, 98.26% region coverage)**. Alle Kern-Pfade, Fehlerpfade, Hot-Reload-, Concurrency- und NaN-Handling-Funktionen sind erschöpfend durch automatisierte Tests abgedeckt.
2. **Unsafe-Code Invariante:** **PASSED (100% Zero-Unsafe)**. Das Crate enthält genau **0** `unsafe`-Blöcke.
3. **NaN-Safety & Upstream Corruption Protection:** **PASSED**. `select_profile_from_chunks` filtert `NaN`/`Inf`-Relevanzwerte konsequent heraus. Falls sämtliche Chunks korrupte Relevanzwerte (`NaN`/`Inf`) enthalten, protokolliert der Router einen `tracing::error!` und gibt einen sauberen `MemFuseError::NotFound` zurück, anstatt panikartig ababstürzen oder indifferente Profil-Entscheidungen zu treffen.
4. **Hot-Reload & Thread Safety:** **PASSED**. Profile werden via `parking_lot::RwLock` verwaltet und beim Routing-Aufruf atomar als Snapshot geklont. 20-fache parallele Lese- und Schreibzugriffe auf `RouterEngine` laufen stressgetestet ohne Race-Conditions, Poisoning oder Deadlocks.
5. **DAG-Architektur Invariante:** **PASSED**. `memfuse-router` importiert nur Layer 0 (`memfuse-core`), Layer 1 (`memfuse-store`), Layer 2 (`memfuse-db`) sowie `memfuse-ollama`. Keine Aufwärts-Importe.
6. **Refactoring & Optimierung:** **PASSED**. `EntityId::from_doc_id(chunk.doc_id)` wird direkt aus den transformierten ContextChunks abgeleitet, wodurch redundantes String-Rehashing über `EntityId::from_key` sowie nicht erreichbare Fehlerzweige eliminiert wurden.

---

## 2. Unsafe Code-Inventar

- `grep -rn "unsafe" crates/memfuse-router/src/`
- **Anzahl `unsafe`-Blöcke:** **0** (Hartes Kriterium erfüllt).

---

## 3. Dependency & Security Audit

```bash
cargo audit -p memfuse-router
```
**Ergebnis:** 0 Schwachstellen, 0 RUSTSEC-Warnungen.

---

## 4. Testabdeckung & Verifikation

```text
Filename                      Regions    Missed Regions     Cover   Functions  Missed Functions  Executed       Lines      Missed Lines     Cover
---------------------------------------------------------------------------------------------------------------------------------------------------
dispatch.rs                        57                 2    96.49%           6                 1    83.33%          56                 1    98.21%
profile.rs                         36                 0   100.00%           3                 0   100.00%          49                 0   100.00%
router.rs                         195                 3    98.46%          14                 0   100.00%         140                 1    99.29%
---------------------------------------------------------------------------------------------------------------------------------------------------
TOTAL                             288                 5    98.26%          23                 1    95.65%         245                 2    99.18%
```

### Abgedeckte Edge Cases:
- Determinismus bei identischen Profile-Scores (Tie-Breaking wählt stets den niederen Profil-Index).
- Verkraftung von 1 bis 50 parallel geladenen SLM-Profilen.
- Truncated ContextWindow Dispatch an HTTP JSON-RPC 2.0 Mock-Server.
- NaN/Inf Handling bei einzelnen und allen Chunks.
- Concurrent Hot-Reloading unter hoher paralleler Routing-Last.
- Fehlerhafte HTTP- und RPC-Antworten (HTTP 500, RPC Error Codes, fehlendes Result/Error).
- Leere Suchergebnisse und nicht konfigurierte SLM-Profile.
- Behandlung ungültiger/korrupter Suchergebnisse und direkter `EntityId`-Derivierung aus `DocId`.

---

## 5. Session Log & Verification (2026-09-04)
- **Compilation & Warmup Threshold Fix**: Resolved compilation errors in `router.rs` by implementing `ProfileScoring` and `score_profile()`. Replaced deprecated `hybrid_search_with_strategy` with `collection.query()`. Unified conformal calibration warmup window total threshold using `CALIBRATION_WARMUP_SAMPLES` constant (30 samples) across `select_profile_cascade`, `ConfidenceMetrics`, and unit tests, resolving `AGT-ROUTER-2db4f208`.
- **NaN-Safety & Test Expansion**: Enforced non-finite `query_embedding` validation and expanded test suite from 42 to 48 tests covering ConformalCalibrator defaults, empty MCP endpoints, non-finite query vector rejection, and profile reset utilities.
- **Verification**: All 48 unit/integration tests in `memfuse-router` pass reliably (`cargo test -p memfuse-router --all-features`). Zero clippy warnings, zero formatting diffs, zero unsafe blocks. Workspace check `cargo check --workspace --exclude memfuse-tauri` succeeds cleanly.

---

## 6. Audit Findings & Chaos Engineering Report (2026-09-04)

### Audit Findings

| ID | Severity | Category | File & Location | Description | Status |
|---|---|---|---|---|---|
| `AGT-ROUTER-2db4f208` | MAJOR | LOGIC | `src/router.rs:298` | Calibration warmup window total threshold in `select_profile_cascade` unified using `CALIBRATION_WARMUP_SAMPLES` (30 samples) across engine and tests. | RESOLVED |
| `AGT-ROUTER-19a753f1` | MINOR | SMELL | `src/tests.rs:1510` | `test_calibrated_threshold_convergence` iteration count aligned with 30-sample threshold requirement (55 calls). | RESOLVED |

### Chaos Engineering Report

| Scenario | Result | Recovery Behavior | Findings |
|---|---|---|---|
| Crash mid-write | OK | Stateles / In-memory routing logic; no direct disk persistence | — |
| Disk-Full ENOSPC | OK | Stdio IPC buffer writes return controlled `MemFuseError::Internal` error | — |
| OOM / Backpressure | OK | ContextWindow trimming enforces bounded memory via `TokenBudget` | — |
| SIGBUS mmap-truncate | N/A | `memfuse-router` does not use `mmap` or unsafe memory mapping | — |
| SIGKILL recovery | OK | Stateless engine re-initializes from `Collection<LsmStorage>` on restart | — |
