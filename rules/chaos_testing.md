# Chaos Testing Rules — WAL V3/MVCC Fault-Injection

> Referenziert aus `AGENTS.md §7` und `DECISIONS.md` (ADR-062)

## Szenario-Übersicht

| Szenario | Beschreibung / Umsetzung in MemFuse |
|---|---|
| `TaskMassacre` | Echte `JoinHandle::abort()` gegen konkurrierende Writer auf `LsmStorage` |
| `BitFlipInjection` | SSTable-Block/Bloom/Index-CRC Bit-Flips via direkte Dateimanipulation |
| `PowerCutSimulation` | Echter Prozess-Kill (SIGKILL) via Subprozess `examples/chaos_writer.rs` |
| `MemoryExhaustion` | Generiert Speicherdruck unter Nutzung des realen `ResourceTracker`/`LsmConfig`-Budgets |
| `DroppedWrite` | Simuliert I/O-Fehler auf Dateisystem-Ebene via `chmod`-Read-Only-Berechtigungen |
| `ConcurrentWriteFlood` | Konkurrierende Schreiber-Tasks fluten `LsmStorage` gleichzeitig unter knappem Budget |
| `CombinedChaosMatrix` | Kombinierte Ausführung mehrerer Fault-Szenarien in randomisierter Reihenfolge |

Explizit verworfene Szenarien:
- `IOLatency` (kein belegter Slow-Disk-Use-Case)
- `NetworkDegradation` (keine Netzwerkschicht in MemFuse, siehe ADR-010)

## Kernregeln für Chaos-Tests

### 1. Unabhängige Ground-Truth-Referenz
Jeder Chaos-Test MUSS einen von der Implementierung unabhängigen Ground-Truth-Wert verwenden (siehe `rules/testing.md` Anti-Test-Mirroring), z.B. eine externe Log-Datei mit tatsächlich geschriebenen Werten VOR jedem Schreibversuch, NICHT eine aus dem WAL selbst rekonstruierte Erwartung.

### 2. Isolierter Test-Scope
Diese Testsuite darf ausschließlich `tests/` und `examples/` in `crates/memfuse-store` verändern. Jede Änderung an `src/` im Rahmen dieser Suite erfordert eine eigene, separate ADR.

## CI-Ausführung

- Einzeltests (`chaos_power_cut`, `chaos_task_massacre`, `chaos_bitflip_sstable`, `chaos_dropped_write`, `chaos_memory_pressure`) laufen als reguläre Integrationstests bei jedem `cargo test --workspace`.
- Die kombinierte Fault-Matrix (`crates/memfuse-store/tests/chaos_matrix.rs`) ist `#[ignore]`-gated, wird lokal über `just chaos-test` aufgerufen und läuft automatisch im nightly CI-Workflow (`.github/workflows/chaos.yml` per `schedule` und `workflow_dispatch`). Sie ist ausdrücklich NICHT in `on: pull_request` eingebunden und blockiert keine PRs.
- Bei jedem Testlauf wird der verwendete Seed GELOGGT. Im Falle eines CI-Fehlschlags kann der Test mit `CHAOS_SEED=<seed> just chaos-test` exakt lokal reproduziert und debuggt werden.
