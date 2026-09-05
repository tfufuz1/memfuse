# SPEC-032: Resource Budget Enforcement
> **Status:** 🟢 DONE | **Priorität:** P0 | **Crate(s):** chimera-core | **Kontext:** MILITARY / ROBOTICS

## 0. Threat Analysis
Memory Pressure Mechanismen (aus SPEC-025) brauchen Harte Limits in Config.

## 1. Problem (IST-Zustand)
Keine limits auf Caches (`moka`), Collections und Index-Größen.

## 2. Anforderungen (SOLL)
### Funktionale Anforderungen
- FR-1: Config liefert globale harte Budgets (z.B. 2GB max).
- FR-2: Allocation an PageCache, MemTable und HNSW im Verhältnis verteilen.
- FR-3: Lock-freie Inkrementierung/Dekrementierung via Atomics.
- FR-4: Peak-Memory Tracking für Post-Mortem Analyse.

## 3. Implementierung
### 3.1 `ResourceTracker` (chimera-core)
- Nutzt `AtomicU64` für `memory_used`, `memory_peak` und `cpu_cycles_used`.
- `consume_memory(bytes)`: Verwendet Compare-And-Swap (CAS) Loop, um das Limit strikt einzuhalten ohne Mutexe zu nutzen.
- `release_memory(bytes)`: Sicherer Abzug mit Sättigung bei Null.
- `status()`: Liefert `Normal`, `Stall` (80%) oder `Reject` (95%) Status für Backpressure-Logik.

## 7. Akzeptanzkriterien
- [x] `ResourceBudget` Struct überall injiziert (Core, Storage).
- [x] Lock-freie Durchsetzung der Limits verifiziert.
- [x] Integration mit `chimera-metrics` für Visualisierung des Budgets.
