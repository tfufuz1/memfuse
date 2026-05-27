# MemFuse Master Specification (The Single Source of Truth)

> **Status:** 🔴 ACTIVE - Pflichtlektüre für ALLE Coding Agenten (Jules Accounts)
> **Autor:** Lead Architect
> **Zertifizierung:** Dieses Dokument überschreibt JEDWEDE historische Spec (frühere v1/v2 Versionen, comprehensive_specification usw.). Es ist die absolute und einzige "Master Specification".

---

## 🎯 1. Produktvision ("The Why")
MemFuse ist das **"SQLite für KI-Agenten"**. Eine eingebettete, in-process, hochperformante Hybrid-Search-Vektordatenbank, die lokal betrieben wird. Sie eliminiert die Komplexität von Cloud-Vector-DBs für lokale LLM-RAG-Systeme.

### 🛡️ Sovereign Core Doctrine (Unverhandelbare Gesetze)
1. **Zero-Panic Policy:** Absolut kein `.unwrap()`, `.expect()` oder `panic!()` außerhalb von Testcode. Alle Fehler werden über `memfuse_core::MemFuseError` via `?` propagiert.
2. **Async-Only I/O:** Keine blockierenden Aufrufe wie `std::fs`. Es wird ausschließlich `tokio::fs` und `tokio::io` verwendet.
3. **Safe Rust Isolation:** `#![forbid(unsafe_code)]` gilt weitreichend. `unsafe` nur in `memfuse-index/distance.rs` für SIMD (immer mit `// SAFETY:` Begründung).
4. **Triple-Test-Gate:** Kein Code-Merge ohne 3x erfolgreiche Testläufe hintereinander, 0 Clippy-Warnungen und sauberen `debt-audit`.

---

## 🏗️ 2. System-Architektur (Crate-Hierarchy)
MemFuse besteht aus exakt **11 Crates** und folgt einem strikten Directed Acyclic Graph (DAG) Modell. Abhängigkeiten zeigen nur nach unten. Zyklische Abhängigkeiten sind untersagt (CI-Breaker).

| Layer | Crate | Rolle | Status |
|:---|:---|:---|:---|
| **L3** | **`memfuse-py`** | Interface. PyO3/NumPy Python-Bindings. | ✅ Stabil |
| **L2** | **`memfuse-db`** | Orchestrator & Facade für Collections, Hybrid-Search (RRF). | ✅ Stabil |
| **L2** | **`memfuse-checkpoint`** | Snapshot Registry & Time-Travel MVCC. | 🛑 FROZEN |
| **L2** | **`memfuse-sandbox`** | WASM Tool Sandbox & Token-Budgeting. | 🛑 FROZEN |
| **L2** | **`memfuse-saos-agent`**| Task/Workflow Engine Orchestration. | 🛑 FROZEN |
| **L1** | **`memfuse-store`** | Sub-Engine: LSM-Tree, WAL, MemTables, SSTable Persistenz. | ✅ Stabil |
| **L1** | **`memfuse-index`** | Sub-Engine: HNSW Graph für Vektorsuche inkl. SQ8 Quantisierung. | ✅ Stabil |
| **L1** | **`memfuse-text`** | Sub-Engine: BM25 Engine & Inverted Index, Morphologie. | ✅ Stabil |
| **L1** | **`memfuse-graph`** | Sub-Engine: CSR-Graph für Entity-Relation. | 🛑 FROZEN |
| **L1** | **`memfuse-crypto`** | Sub-Engine: AES-GCM Encrypt/Decrypt (at-rest, WAL). | ✅ Stabil |
| **L0** | **`memfuse-core`** | Kernel. Shared Types, Traits, Errors, TxBuffer. Importiert niemanden. | ✅ Stabil |

---

## 🛤️ 3. Roadmap & Work Packages (WPs)

### Phase 1: Foundation
- **WP-0.0: Dependency Audit** — Zero-Panic & Async-Safety Refactoring.
- **WP-1.1: Background Compaction** — Automatisierte SSTable-Konsolidierung.
- **WP-1.2: Collections** — Namespace-Isolation und API-Facade.
- **WP-1.3: Atomic Commit** — Transaktionssicherheit über Store & Index.

### Phase 2: Search & Retrieval
- **WP-2.1: Hybrid Search** — Integration von BM25 und Reciprocal Rank Fusion (RRF).
- **WP-2.2: Scalar Quantization (SQ8)** — 75% RAM-Reduktion für Vektoren.

### Phase 3: Interface & Security
- **WP-3.1: Python Bindings** — Volle Unterstützung für lokale Agenten-Frameworks.
- **WP-3.2: Encryption at Rest** — Data at rest encryption.

### Phase 4: Hyper-Scale
- **WP-4.1: Memory-Mapped I/O** (mmap)
- **WP-4.2: Advanced Filtering**
- **WP-4.3: DiskANN Out-of-Core**

### Phase 5, 6, 7: SAOS, Goldstandard & Connectivity [FROZEN]
- SAOS Work Packages (WP-5.x), GS-Funktionen (WP-6.x) und RAG Pipeline (WP-7.x) sind strukturell eingefroren. Der Kernfokus liegt auf der Perfektionierung von L0-L1-L2 (dem Sovereign Core) als robuste Vector Database.

---

## 🤖 4. Multi-Agent Orchestration
Die Codebase wird von einem autonomen Team aus **13 JULES-Agenten** stetig weiterentwickelt. Das System ist nach dem *Conveyor Belt* (Fließband) Modell organisiert.
Jeder Entwicklungsschritt bedingt "Spec-Driven Development": Die `docs/specs/...` spezifiziert das Feature, gefolgt von TDD-Implementierung und Refactoring in Übereinstimmung mit der Sovereign Core Doctrine.

*End of Spec - Agent, please acknowledge and proceed.*
