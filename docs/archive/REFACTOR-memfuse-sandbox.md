# REFACTOR-PLAN: memfuse-sandbox
**Datei:** `docs/specs/REFACTOR-memfuse-sandbox.md`
**Erstellt:** 2026-05-28
**Priorität:** HIGH
**Geschätzter Aufwand:** 2 Tage
**Voraussetzung:** memfuse-core, memfuse-db

---

## CRATE-ZUSTAND: EHRLICHE BEWERTUNG

| Dimension          | Aktuell       | Ziel          |
|--------------------|---------------|---------------|
| Panic-Freiheit     | 100%          | 100%          |
| Skeleton-Anteil    | Hoch (Mock)   | 0             |
| Test-Coverage      | Enforced (AC) | >90%          |
| API-Vollständigkeit| Fehlend       | 100%          |
| Algo-Korrektheit   | OK (Limiter)  | VERIFIED      |

---

## IDENTIFIZIERTE SCHWACHSTELLEN

### BLOCKING (Release-Blocker — SOFORT)

#### FIND-SBX-001: Skeleton Implementierung in Host-Funktionen (WP-6)
**Typ:** Business Logic / Incomplete Feature
**Datei:** `crates/memfuse-sandbox/src/host_functions.rs`
**Problem:** Die WASM-Host-Funktionen `db_search`, `db_insert` und `db_get` leiten keine reellen Lese-/Schreibzugriffe an das System weiter. Sie beinhalten ein `// TODO(WP-6): Actual orchestrator L2 loopback` und returnen hartkodiert `0`. Agentic Tools innerhalb des Sandbox-Environments sind dadurch völlig isoliert, können aber nicht mit MemFuse sprechen und keine RAG-Aufgaben erledigen.
**Auswirkung:** Tools im Sandbox-Mode können nicht auf die Vector-Datenbank zugreifen. Dies bricht den Use-Case von sicheren Sandboxed-Agents.

**Refaktorisierungsanweisung:**
```
1. Die Argumentliste der Callbacks muss von dem `Caller::data()` (welches `SandboxState` ist) Gebrauch machen, um einen Channel (z.B. `tokio::sync::mpsc`) zum saos-agent zu erhalten.
2. Der Orchestrator sendet via Channel einen asynchronen Intent an die `MemFuse` Instanz. Da `Caller` jedoch synchron ist bei `wasmtime`-Func-Wraps (ohne async-Wasmtime), muss das Architekturdesign evaluiert werden (z.B. `wasmtime::Func::wrap_async`).
```

**Akzeptanzkriterien:**
- [ ] `db_search`, `db_insert` und `db_get` rufen via Async-Wasmtime `MemFuse::search`, `MemFuse::insert`, etc. auf.

#### FIND-SBX-002: AirGapVerifier ist ein Mock (WP-6.6)
**Typ:** Security / Compliance
**Datei:** `crates/memfuse-sandbox/src/airgap.rs`
**Problem:** `AirGapVerifier::verify` gibt immer statisch `Ok(AirGapReport { network_isolated: true, ... })` zurück und enthält einen Kommentar `// TODO(WP-6.6): Implement actual verification`. Dies gaukelt einem Sovereign-Deployment Compliance vor, wo keine validiert wird.
**Auswirkung:** Kritische Security-Vulnerability bei Enterprise-Deployments, die auf Air-Gap Proofs angewiesen sind.

**Refaktorisierungsanweisung:**
```
1. Implementiere reale OS-spezifische Checks. Mindestens unter Linux über `/proc/self/fd` schauen, ob Socket-Filedescriptoren (family != AF_UNIX) existieren und offen sind.
2. Prüfe, ob Verschlüsselung konfiguriert (Encryption aktiviert in Storage) ist, bevor `encryption_active: true` gemeldet wird.
```

**Akzeptanzkriterien:**
- [ ] Der AirGapVerifier wirft einen Fehler, wenn das Programm nachweislich einen offenen TCP/UDP Socket hat.

---

## REFAKTORISIERUNGSREIHENFOLGE

1. FIND-SBX-001 (Host-Functions auf Async-Wasmtime umstellen und mit MemFuse koppeln).
2. FIND-SBX-002 (AirGap Verifier funktional machen).

## DONE-DEFINITION FÜR DIESES CRATE
- [ ] Keine `TODO(WP-6)` mehr in den Host-Funktionen.
- [ ] AirGapReport basiert auf Laufzeitüberprüfungen.
- [ ] `just triple-test` grün.
