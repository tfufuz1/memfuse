# MemFuse Master Specification (The Single Source of Truth)

## 🎯 1. Produktvision & Core Doctrine
MemFuse ist das **"SQLite für AI Agents"**. Eine eingebettete, hochperformante Hybrid-Search-Vektordatenbank, die lokal betrieben wird. Sie ist so konzipiert, dass sie die Komplexität von Vektorsuche (HNSW), Volltextsuche (BM25) und State-Management für LLM-Agenten abstrahiert.

### 🛡️ Sovereign Core Doctrine (Absolut Verbindlich)
1.  **Zero-Panic Policy:** Absolut kein `.unwrap()`, `.expect()` oder `panic!()` außerhalb von Testcode. Alle Fehler werden über `memfuse_core::MemFuseError` propagiert.
2.  **Async-Only I/O:** Keine blockierenden Aufrufe wie `std::fs`. Es wird ausschließlich `tokio::fs` und `tokio::io` verwendet.
3.  **Safe Rust Isolation:** `unsafe` ist verboten, außer in `memfuse-index/distance.rs` für SIMD (immer mit `// SAFETY:` Begründung).
4.  **Triple-Test-Gate:** Kein Code wird gemerged, ohne 3x hintereinander erfolgreiche Testläufe, 0 Clippy-Warnungen und einen erfolgreichen `debt-audit`.

---

## 🏗️ 2. System-Architektur (Crate-Hierarchy)
MemFuse folgt einem strikten Directed Acyclic Graph (DAG) Modell.

| Crate | Rolle | Status |
|:---|:---|:---|
| **`memfuse-core`** | Kernel (Types, Error, TxBuffer, Snapshots). Keine Abhängigkeiten. | ✅ Stabil |
| **`memfuse-store`** | Storage (LSM-Tree, WAL, SSTables, Compaction). | ✅ Stabil |
| **`memfuse-index`** | Vector Engine (HNSW Graph, SIMD, SQ8). | ✅ Stabil |
| **`memfuse-text`** | Keyword Engine (BM25 Inverted Index). | 🟡 Skeleton |
| **`memfuse-db`** | Orchestrator & Facade (Collections, Hybrid-Search RRF). | ✅ Stabil |
| **`memfuse-py`** | Python Bindings (PyO3). | 🟡 Partial |
| **`memfuse-orchestrator`** | Agent Workflow Engine (StateGraph). | 🟡 Skeleton |
| **`memfuse-runtime`** | Sandbox Execution (Wasm). | 🟡 Skeleton |

---

## 🛤️ 3. Roadmap & Work Packages (WPs)

### Phase 1: Foundation (Abgeschlossen/In Arbeit)
- **WP-0.0: Dependency Audit:** Zero-Panic & Async-Safety Refactoring.
- **WP-1.1: Background Compaction:** Automatisierte SSTable-Konsolidierung.
- **WP-1.2: Collections:** Namespace-Isolation und API-Facade.
- **WP-1.3: Atomic Commit:** Transaktionssicherheit über Store & Index.

### Phase 2: Intelligence & Retrieval (Priorität)
- **WP-2.1: Hybrid Search:** Integration von BM25 und Reciprocal Rank Fusion (RRF).
- **WP-2.2: Scalar Quantization (SQ8):** 75% RAM-Reduktion für Vektoren.
- **WP-3.1: Python Bindings:** Volle Unterstützung für lokale Agenten-Frameworks.

### Phase 3: SAOS Goldstandard (Zukunftsvision)
- **GS-01: 4-Signal Fusion API:** Native Verschmelzung von Vektor, Text, Graph und Metadaten.
- **GS-02: Declarative StateGraph API:** Eingebauter Workflow-Orchestrator (LangGraph Ersatz).
- **GS-03: Autonomes Kontext-Management:** Proaktive Context-Injection für LLMs.
- **GS-07: Kryptografische WAL-Verifikation:** Auditierbare Audit-Logs.

---

## 🔍 4. Audit & Health Check (Status Mai 2026)
Basierend auf dem letzten Audit (`ALG-WEAKNESS-REGISTER.md`):

### Kritische Offene Punkte (S2/S3)
1.  **ALG-D1-007 (WAL CRC32):** WAL-Einträge werden bei Replay nicht verifiziert. Gefahr von stiller Datenkorrumpierung.
2.  **ALG-D1-011 (WAL GC):** Alte WAL-Dateien werden nach Flush nicht gelöscht. Unendliches Disk-Wachstum.
3.  **ALG-D1-008 (Scan Prefix):** Inkonsequenter `seq_no` Vergleich bei Prefix-Scans in Immutable MemTables.
4.  **ALG-D6-003 (Commit Order):** Index wird vor Storage committet. Crash-Gefahr führt zu Phantom-Ergebnissen.

---

## 🤖 5. Agent Squad & Entwicklungsprotokoll
MemFuse wird von 14 spezialisierten Agenten (JULES Accounts 00-13) gewartet.

### Der TDD-Loop für Agenten:
1.  **`just debt-audit`**: Prüfen auf technische Schulden.
2.  **Atomic Spec**: Erstellen der Spezifikation in `docs/specs/`.
3.  **Red Phase**: Fehlschlagenden Test in `tests/` oder `src/` schreiben.
4.  **Green Phase**: Implementierung unter Einhaltung der *Sovereign Core Doctrine*.
5.  **Triple-Test**: `just triple-test` ausführen.

---

## 📈 6. Performance Targets
- **Latenz:** < 10ms für Vektor-Search (100k Records).
- **Speicher:** < 200MB Baseline RAM für 1M quantisierte Vektoren.
- **Sicherheit:** 100% Rust Safe-Code in allen I/O Pfaden.

---
> **Disclaimer:** Dieses Dokument ist die Single Source of Truth für alle Architektur-Entscheidungen. Abweichungen müssen explizit als ADR (Architectural Decision Record) dokumentiert werden.
