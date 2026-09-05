# SPEC-025: Memory-Pressure & OOM-Resilience
> **Status:** IMPLEMENTED | **Priorität:** P0 | **Crate(s):** chimera-storage, chimera-sync | **Kontext:** MILITARY EDGE

## 0. Threat Analysis
1. Ein Edge-Node hat 2GB RAM Limit. HNSW-Index Updates und parallele Compaction treiben Speichernutzung auf 2.1GB -> OS OOM Killer tötet Prozess.

## 1. Problem (IST-Zustand)
MemTable wächst ohne Gegendruck, wenn Compaction nicht hinterherkommt. `TxBuffer` sammelt Transaktionen von abgestürzten Agenten ewig (Memory Leak). Kein Load-Shedding bei Memory-Pressure.

## 2. Anforderungen (SOLL)
### Funktionale Anforderungen
- FR-1: Globale `ResourceTracker` (Atomic) aus `chimera-core` (SPEC-032), jede Allokation registriert sich hier.
- FR-2: Write-Stall, sobald `mem > limit * 0.8` (Throttling). Hard-Reject sobald `mem > limit * 0.95`.
### Nicht-Funktionale Anforderungen
- NFR-1: Korrektheit: `TxBuffer` bereinigt nach einer konfigurierbaren TTL Orphans automatisch (GC Task).
- NFR-2: Latenz: Write-Stall verzögert asynchrones `flush()`.

## 3. Implementierung
### 3.1 `ResourceTracker` Integration
`LSMStorage` verwendet einen `Arc<ResourceTracker>` zur Überwachung des aktiven Heaps (MemTable + WAL Replay).
- `on_commit`: Berechnet das Volumen neuer Daten und prüft das Budget vor der Allokation.
- `flush`: Gibt das MemTable-Budget frei, sobald die Daten persistiert wurden.

### 3.2 Metriken (SPEC-033)
Alle Budget-Änderungen werden über `ChimeraMetrics::record_memory_pressure` an Prometheus gemeldet, was das Debugging von Leaks und OOM-Situationen im Feld ermöglicht.

### 3.3 Fehlerbehandlung
`ChimeraError::BudgetExceeded` (Hard-Reject) oder `ChimeraError::Backpressure` (Soft-Stall).

## 4. Sicherheits-Invarianten
- @REQUIRE: System-Konfig `max_ram_mb` muss für den LSM-Pfad gesetzt sein.
- @ENSURE: Atomarer Release nach Flush verhindert Doppelanrechnung.
- @NEVER: Registrierung von Allokationen darf den Schreibpfad blockieren (außer bei gewolltem Stall).

## 7. Akzeptanzkriterien
- [x] Orphan-Cleanup für TxBuffer via Timeout.
- [x] MemTable Soft-Stall und Hard-Reject integriert in `chimera-storage`.
- [x] Integration mit `chimera-metrics` für Echtzeit-Pressure-Monitoring.
