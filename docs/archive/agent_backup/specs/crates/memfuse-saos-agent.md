# CRATE-SPEC: memfuse-saos-agent
**Version:** CURRENT -> TARGET (Gold Standard)
**Status:** NEEDS_REDESIGN

---

## SINGLE RESPONSIBILITY
Diese Crate ist EXAKT zuständig für: Bereitstellung der Kerninfrastruktur für den Agentic Database Support um memfuse-saos-agent.

## VOLLSTÄNDIGE PUBLIC API (Soll-Zustand)
Strikt isolierte API-Verträge. Spezifische Traits und Typen gemäss Architektur.
API muss deterministisch antworten. Alle Fehler sind explizit gemapped (`MemFuseError`).

## KRITISCHE INVARIANTEN (NIEMALS VERLETZEN)
- INVARIANT-01: Zero-Panic bei I/O oder Execution Workloads
- INVARIANT-02: Skalierbarkeit für M+ Einträge mit festen RAM-Budgets
- INVARIANT-03: strikter `unsafe` Code Ausschluss (mit HNSW SIMD als einziger markierter Ausnahme).

## IDENTIFIZIERTE SCHWACHSTELLEN
- SK-03: Potenziell instabile Platzhalter (siehe FORENSIC_FINDINGS.md)
- RA-01: Verborgene `unwrap()` Calls.

## KONKRETE HANDLUNGSANWEISUNGEN FÜR IMPLEMENTIERER
### PRIORITÄT 1 — SOFORT (Release-Blocker)
1. Elimination von panic!-Möglichkeiten, robustes State Management.

### PRIORITÄT 2 — KURZFRISTIG (Pre-Launch)
1. Hinzufügen von Property-Based Tests zur Verifikation des States.

### PRIORITÄT 3 — MITTELFRISTIG (Post-Launch)
1. Observability (`tracing`) komplett integrieren.

## TESTABDECKUNGS-ANFORDERUNGEN
- Unit-Tests: Alle public / critical intern functions
- Integration-Tests: Workflow testing in DB layer
- Property-Tests (proptest): Serialisierung / Deserialisierung
- Benchmarks: Hot Path Analysis

## SCHNITTSTELLEN ZU ANDEREN CRATES
[DAG Rule]: Nur Referenzierungen nach unten (Layer 3 -> Layer 0) gestattet.
