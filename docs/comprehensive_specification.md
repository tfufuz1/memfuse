# MemFuse — Comprehensive Master Specification

Dieses Dokument bietet eine vollständige, top-down spezifizierte Übersicht des MemFuse-Projekts. Es wurde so konzipiert, dass auch neue LLM-Agenten die Architektur, Ziele und Entwicklungsregeln des Projekts vollständig und kontextfrei erfassen können. Die Spezifikation wird systematisch entlang des Systemfortschritts erweitert.

---

## 1. Produktvision ("The Why")
MemFuse ist das **"SQLite für KI-Agenten"**. Es ist eine in-process, einbettbare und performante Vektor- und Hybrid-Suchdatenbank, die lokal betrieben wird. Es eliminiert die Abhängigkeit von Cloud-gebundenen Vector-DBs oder komplexen Client-Server-Architekturen in lokalen LLM-RAG-Systemen.

**Fokus-Bereiche:**
* **Lokaler Betrieb:** In-Process-Abarbeitung (z.B. eingebettet in Python via PyO3).
* **High Performance:** SIMD-optimierte Graphensuche (HNSW) und Log-Structured Merge Trees (LSM).
* **Zero-Panic & Safety:** Absolut speichersicher und crash-resistant ("Sovereign Core Doctrine").

---

## 2. Kernziele und strategische Prioritäten (Objectives & Priorities)

Um die Produktvision zu realisieren, verfolgt MemFuse klare, kompromisslose Ziele und eine strikt priorisierte Ausführungsreihenfolge.

### 2.1. Die strategischen Kernziele (Core Objectives)
1. **Absolute Autarkie (Sovereign AI):** MemFuse strebt nach 100% lokaler Offline-Fähigkeit (Air-Gapped Deployment). Es darf keine externen Netzwerkschnittstellen für Embedding-Modelle zwingend voraussetzen.
2. **"Zero-Panic" Ausfallsicherheit:** Das System darf unter keinen Umständen abstürzen. Memory Leaks, Race Conditions und Panics (`unwrap()`, `expect()`) sind auf Compile-Ebene verboten. Das Backend basiert deshalb auf Rust.
3. **Agentic Native (Für LLMs gebaut):** Externe Frameworks wie LangChain oder CrewAI sollen durch native On-Disk-Features obsolet gemacht werden. MemFuse integriert Routing, State-Management (Declarative StateGraph) und Kontext-Management direkt auf Datenbank-Ebene.
4. **Multi-Signal Retrieval (Hybrid RAG):** Verschmelzung von semantischer Vektorsuche, lexikalischer Keyword-Suche (BM25), Graphen-Traversierung und Metadaten-Filtern in einer atomaren 4-Signal Fusion API mit deterministischem RRF-Scoring.
5. **Autonome Systementwicklung (SAOS / Conveyor Belt):** Das Codebase wird nicht nur von Menschen, sondern fortlaufend von rudel spezialisierter LLM-Agenten ("JULES") im Multi-Agent-Fließbandverfahren weiterentwickelt und gehärtet.

### 2.2. Globale Ausführungsprioritäten (Execution Priority List)
Entwicklungsaufgaben müssen **stets** nach folgender Reihenfolge abgearbeitet werden. Priority 1 blockiert Priority 2, usw.:

1. **🔴 PRIO 1: Tech-Debt Elimination & Invariant Verification (WP-0.0)**
   *Die Behebung von technischen Schulden hat immer absolute Priorität vor neuen Features.*
   Dazu zählt das Verhindern von unerlaubten `unwrap()`-Aufrufen, das Beheben von Clippy-Warnungen und die Sicherstellung strikter asynchroner I/O (`tokio::fs` statt `std::fs`).
2. **🟠 PRIO 2: Storage & Index Stabilität (WP-1.x)**
   Sicherstellung der LSM-Tree Background Compaction ohne Memory Leaks und atomares WAL (Write-Ahead-Logging). Die Speicherschicht (`memfuse-store`) darf unter keinen Umständen Daten korrumpieren.
3. **🟡 PRIO 3: Performance & Retrieval Quality (WP-2.x)**
   Einführung performanter Hybrid-Search-Mechanismen (BM25 Inverted Text Index) sowie SIMD-basierten SQ8-Quantisierungen für drastische Speicheroptimierungen.
4. **🔵 PRIO 4: Ergonomie, Interface & Zukunftsskalierung (WP-3.x -> WP-6.x)**
   Schaffung von Python Bindings (`memfuse-py`), Security (Encryption at Rest), Memory-mapped I/O Operations und abschließend die Erschließung der "Goldstandard"-APIs (StateGraph, Morphologische Inferenz-Optimierung, Multi-Agent Namespaces).

---

## 3. Die Crate-Architektur (DAG Model)
Das Projekt besteht aus strikt hierarchischen Rust-Crates. Abhängigkeiten sind ein Directed Acyclic Graph (DAG) – zyklische Abhängigkeiten sind untersagt.

### Level 0: Interface Layer
* **`memfuse-py` (WP-3.1):** Das primäre Interface. PyO3/Maturin-basierte Python-Bindings.
* **`memfuse-crypto` (WP-3.2):** Encryption at Rest (AES-GCM / ChaCha20) für Security Compliance.

### Level 1: Orchestration Layer
* **`memfuse-db` (WP-1.2, WP-4.2):** Der zentrale Orchestrator. Verbindet Storage und Indices. Handhabt namespaces (Collections) und ist verantwortlich für die Hybrid-Search Facade inkl. Reciprocal Rank Fusion (RRF).

### Level 2: Sub-Engines (Isolation: no direct comms)
* **`memfuse-store` (WP-1.1, WP-4.1):** Die Storage- und Persistenzschicht. Log-Structured Merge Tree (LSM) Architektur mit Write-Ahead-Logs (WAL) und Background Compaction. Zukünftig Memory-Mapped I/O.
* **`memfuse-index` (WP-2.2, WP-4.3):** Die Vector-Engine. HNSW-Graph-basiert für Approximate Nearest Neighbor (ANN). SIMD-Distanzberechnungen und Scalar Quantization (SQ8).
* **`memfuse-text` (WP-2.1):** BM25 basierter Inverted Index für deterministische Schlüsselwortsuche ("Keyword Search").

### Level 3: Shared Kernel (Root Layer)
* **`memfuse-core` (WP-0.0):** Fundamentale Typen (`TxBuffer`, `MemBank`, `Snapshot`). Beherbergt den `MemFuseError` (für Zero-Panic). Dies ist die einzige Crate, die von *allen* importiert wird. Darf selbst keine Abhängigkeiten von Level 1/2 haben.

---

## 4. Sovereign Core Doctrine & Entwicklungsregeln
Das Projekt erfordert extrem hohe Resilienz. Alle Entwicklungen unterliegen diesen absoluten "Non-Negotiable" Vorgaben:

1. **Zero-Panic:** Kein `.unwrap()`, `.expect()` oder `panic!()` in Produktivcode. Fehler müssen via `?` operator als `MemFuseError` propagiert werden.
2. **Keine blockierende I/O (Async Safety):** Alle Storage-Operationen (in `memfuse-store` etc.) müssen async über `tokio::fs` oder `tokio::io` ablaufen. Normale `std::fs` Aufrufe in async-Kontexten sind CI-Breaker.
3. **Safe-Rust (`#![forbid(unsafe_code)]`):** Absolute Code-Sicherheit. Ausnahmen sind ausschließlich für SIMD- und FFI-Grenzen (z.B. in `memfuse-index/distance.rs`) zulässig, die streng durch einen `// SAFETY:` Kommentar begründet sein müssen.
4. **Warnings = Errors:** Alle Warnungen in `cargo clippy -- -D warnings` werden als Compile-Fehler gewertet.
5. **No Broken APIs:** Existierende public APIs dürfen in ihren Signaturen nicht gebrochen werden (Backward Compatibility).

---

## 5. Multi-Agent Orchestration (SAOS "Conveyor Belt")
Die Codebase wird von einem autonomen Team aus 13 JULES-Agenten stetig weiterentwickelt. Das System ist nach dem *Conveyor Belt* (Fließband) Modell organisiert, orchestriert über ein JULES-ANCHOR v2.0 Inline-Kommentarsystem.

* **Agentenstruktur:** 13 spezialisierte Agenten (z.B. "Debt Hunter", "Core Guardian", "Store Engineer").
* **Dynamic Queue Dispatch:** Agenten triggern sich gegenseitig via GitHub Actions in Abhängigkeit vom aktuellen Code-Zustand. Startet bei `develop`-Pushes und wird per `jules-sync-locks.sh` synchron gehalten (`SUCCESSOR:` Anchor Protokoll).
* **Atomic Specs:** Jedes neue Feature erfordert vorher zwingend eine "Atomic Spec" in `docs/specs/SPEC-YYYYMMDD-WP-X.Y-NAME.md` (`just spec [WP]`).

---

## 6. Definition of Done: "Triple-Test-Gate"
Kein Code darf ohne das Passieren qualitätssichernder Gates in `main` gemerged werden (`just triple-test`).

1. **3x Green:** Contract-Tests müssen in exakter Code-Ausführung 3x hintereinander ohne Modifikation durchlaufen. (Verhindern von Heisenbugs).
2. **0 Warnings:** `cargo clippy -- -D warnings` ist grün.
3. **CI-Check:** Der [.github/workflows/jules-quality-gate.yml](file:///home/freddy/Arbeitsplatz/DEV/memfuse/.github/workflows/jules-quality-gate.yml) Workflow ist fehlerfrei.
4. **Tech-Debt Priority:** `just debt-audit` (Zero-Unwrap Scan etc.) muss Prio 1 vor neuen Features haben.

---

## 7. Aktuelle und Zukünftige Feature-Milestones (Roadmap)
Die Implementierung folgt strikten "Work Packages" (WP) und "Goldstandard-Features" (GS):

### Phase 1 & 2: Core, Storage & Security
* **WP-0.0:** Dependency Audit & Zero-Panic Refactoring (Tech-Debt Annihilation).
* **WP-1.1 & WP-1.2:** LSM Background Compaction & Collections/Namespaces.
* **WP-2.1 & WP-2.2:** Hybrid Search (Memfuse-Text BM25) + SQ8 Scalar Quantization RAM-Optimierungen.

### Phase 3: Bindings & Scale (Aktuelle/Zukünftige Sprints)
* **WP-3.1:** Python-Bindings (Eintrittskarte zum User).
* **WP-3.2:** Encryption at Rest.
* **WP-4.x:** Memory-mapped I/O, Out-of-Core Operations (Scale/Filter).

### Phase 4 "The SAOS Goldstandard" (Zukunftsvisionen)
* **GS-01 / WP-6.1:** *4-Signal Fusion API* (Semantisch + Keyword + Graph + Metadaten nativ im Core).
* **GS-02 / WP-6.2:** *Declarative StateGraph API* (Eingebauter Orchestrator zur Ablöse von LangChain/CrewAI).
* **GS-03 / WP-6.3:** *Autonomes Kontext-Management* (Small-to-Big Retrieval & Spatial Fencing in der Datenbank).
* **GS-04 / WP-6.4:** *Multi-Agent Namespaces* (Level-Isolierte Datenbankbereiche für Multi-Agent Access).
* **GS-05 / WP-6.5:** *Morphologische Inferenz-Optimierung* (Tokenizer für Token-Reduktion).
* **GS-06 / WP-6.6:** *Air-Gap Deployment Profile* (100% isolierte Deployments).
* **GS-07 / WP-6.7:** *Kryptografische WAL-Verifikation* (Deterministisch prüfbare Hash-Chains für Logs).

---

> **Hinweis an LLM Agenten:** 
> Bei Ausführung jeglicher Aufgaben ist dieses Dokument als "Single Source of Truth" neben der [ARCHITECTURE.md](file:///home/freddy/Arbeitsplatz/DEV/memfuse/.agent/context/ARCHITECTURE.md) und [SYSTEM.spec.md](file:///home/freddy/Arbeitsplatz/DEV/memfuse/docs/specs/SYSTEM.spec.md) zu verstehen. Jeder Eingriff in die Code-Basis bedingt die Überprüfung gegen die **Sovereign Core Doctrine**.
