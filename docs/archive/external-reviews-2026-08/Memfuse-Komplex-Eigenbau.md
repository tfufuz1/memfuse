# MemFuse Komponenten-Komplexitätsindex

Dieses Dokument bietet eine detaillierte Übersicht aller 15 Workspace-Crates und Kernsysteme des **MemFuse** Projekts, geordnet nach ihrer technischen und architektonischen Komplexität (vom komplexesten zum einfachsten System).

---

## 📊 Komplexitäts-Ranking im Überblick

| Rang | Crate / Komponente | Layer | Hauptverantwortung & Kern-Herausforderung | Komplexitätsgrad |
|---|---|---|---|---|
| **1** | `memfuse-db` | Layer 2 | 4-Signal Fusion Engine, RRF, Multi-Step Query Expansion, Zettelkasten Displacement, MVCC | 🔴 Sehr Hoch (Orchestrierung) |
| **2** | `memfuse-index` | Layer 1 | HNSW Vektorindex, SIMD (AVX2/NEON), DiskANN mmap, Quantisierung | 🔴 Sehr Hoch (Algorithmen & Unsafe) |
| **3** | `memfuse-store` | Layer 1 | Sovereign Pure-Rust LSM-Tree Engine, WAL mit HMAC-Chaining, Zero-Copy Slicing | 🔴 Sehr Hoch (Storage Engine) |
| **4** | `memfuse-graph` | Layer 1 | Compressed Sparse Row (CSR) Graph, Zettelkasten BFS-Traversierung, PPR | 🟧 Hoch (Graph-Theorie) |
| **5** | `memfuse-text` | Layer 1 | Inverted Index, BM25 Search, DACH German Compound Splitter (Morphologie) | 🟧 Hoch (NLP & Text-Indexierung) |
| **6** | `memfuse-agent` | Layer 3 | Zustandsautomaten-Engine, Persistent Workflow, StateGraph, Audit & Crash Recovery | 🟨 Mittel-Hoch (Engine Logic) |
| **7** | `memfuse-crypto` | Layer 1 | HKDF Key Derivation, WAL HMAC Verification, Anti-Tamper & Zeroize drop-semantics | 🟨 Mittel-Hoch (Kryptographie) |
| **8** | `memfuse-checkpoint` | Layer 1 | RAII CheckpointGuard, Snapshot Isolation, Time-Travel Recovery | 🟨 Mittel (System-Zustand) |
| **9** | `memfuse-mcp` | Layer 4 | Stdio JSON-RPC 2.0 Server, Tool-Execution Sandbox, AES-256-GCM-SIV Containment | 🟨 Mittel (Protocol & IPC) |
| **10** | `memfuse-router` | Layer 3 | Query Routing Engine, SLM Profile Dispatch, Dynamic Budget Allocation | 🟦 Moderat (Routing) |
| **11** | `memfuse-embed` | Layer 3 | Cross-Encoder Reranker, ONNX Runtime Feature Gate, Passthrough Fallback | 🟦 Moderat (FFI Integration) |
| **12** | `memfuse-py` | Layer 3 | PyO3 Python Bindings, Tokio-to-GIL Async Runtime Bridge | 🟦 Moderat (FFI Bindings) |
| **13** | `memfuse-tauri` | Layer 4 | Desktop GUI Bridge, Native IPC Event-Handling | 🟩 Leicht-Moderat (UI Backend) |
| **14** | `memfuse-ollama` | Layer 3 | HTTP API Client für lokale Ollama Embeddings & LLM Inference | 🟩 Einfach (HTTP Wrapper) |
| **15** | `memfuse-core` | Layer 0 | Domänen-Typen (`DocId`, `TxId`, `MemoryLink`), Error Handling (`MemFuseError`), Traits | 🟩 Einfach (Fundament / Types) |

---

## 🔍 Detaillierte Analyse der Komponenten (Vom Komplexesten zum Einfachsten)

### 1. `memfuse-db` (Layer 2 — Embedded Engine Core)
* **Komplexitäts-Treiber**:
  * **4-Signal Hybrid Retrieval**: Kombiniert zeitgleich HNSW (Vektor), BM25 (Text), CSR-Graph (Relationen) und strukturierte Metadaten-Filter.
  * **Reciprocal Rank Fusion (RRF)**: Algorithmus zur fairen Gewichtung und Fusion der Ergebnisse unterschiedlicher Suchmodelle.
  * **Post-RRF Zettelkasten Supersedes Displacement (ADR-038)**: Verdrängungslogik in Trefferlisten, falls ein Kandidaten-Dokument ein anderes kandidierendes Dokument explizit via `LinkRelation::Supersedes` überschreibt.
  * **Multi-Step Query Expansion**: Iteratives Query-Rewriting via `MultiStepEngine` in bis zu 3 Runden bei geringer Konfidenz.
  * **MVCC & Transaktions-Sicherheit**: Strikte `TxId`-Alloziierung (`allocate_tx()`) bis `MAX_COLLECTION_SEQUENCE` zur Verhinderung von Race Conditions und Datenkorruption.

### 2. `memfuse-index` (Layer 1 — HNSW & SIMD Vector Engine)
* **Komplexitäts-Treiber**:
  * **HNSW Graph-Algorithmen**: Komplexe hierarchische Graphstrukturen im Arbeitsspeicher für hochdimensionale Nearest-Neighbor-Suche.
  * **Hardware-NAHE SIMD-Optimierung**: Unsafe Rust für Vektor-Distanzberechnungen (Cosine, L2, Dot Product) via AVX2 / ARM NEON.
  * **DiskANN Out-of-Core Persistence**: Memory-Mapped Vector Indexing für Datensätze, die den RAM übersteigen (ADR-017 / ADR-034).
  * **Product Quantization**: Vektorkompression zur RAM-Reduktion bei minimalem Recall-Verlust.

### 3. `memfuse-store` (Layer 1 — Storage Engine)
* **Komplexitäts-Treiber**:
  * **Eigenständige LSM-Tree Architektur**: Pure-Rust Storage-Engine ohne externe C-Bibliotheken (kein RocksDB/SQLite).
  * **WAL mit HMAC-Chaining**: Write-Ahead Log mit kryptographischer Integritätssicherung gegen Stumme Korruption oder Tampering.
  * **Zero-Copy Performance**: Umstellung aller Prefixe und Slices auf `Bytes` (`block_data.slice()`) statt Speicherallokationen per Heap-Copy.
  * **SSTable Block Indexing & Compaction**: Mehrstufiges Mergen und Flush-Management zwischen In-Memory MemTable und immutable SSTable Block-Dateien.

### 4. `memfuse-graph` (Layer 1 — CSR Knowledge Graph)
* **Komplexitäts-Treiber**:
  * **CSR (Compressed Sparse Row) Datenstruktur**: Speicheroptimierte Graphendarstellung für schnelle Kanten-Traversierung in Pure Rust (ohne external Graph-Libs).
  * **Zettelkasten Graph Traversal**: Iterative BFS-Suche mit Zyklenerkennung (`traverse_links()`), limitiert auf `MAX_SEARCH_K`.
  * **Personalized PageRank (PPR)**: Graphbasierte Relevanzgewichtung für Entitäten und Beziehungen im Wissensgraphen.

### 5. `memfuse-text` (Layer 1 — Text & Morphologie Engine)
* **Komplexitäts-Treiber**:
  * **DACH Morphology Engine**: Komplexer Wortsplitter für deutsche Zusammensetzungen (German Compound Splitter) zur Erhöhung des BM25-Recalls.
  * **Contextual Prefix Indexing**: Indexierung von Dokument-Passagen zusammen mit 50–100 Token langen syntaktisch generierten Kontext-Präfixen.
  * **BM25 Inverted Index**: Benutzerdefinierter invertierter Volltext-Index mit TF-IDF / BM25 Scoring.

### 6. `memfuse-agent` (Layer 3 — Workflow & Execution Engine)
* **Komplexitäts-Treiber**:
  * **Persistent StateGraph Engine**: Gerichtet-zyklische und azyklische Workflow-Graphen für langlaufende Agenten-Schleifen.
  * **Crash & Recovery Loop**: Exakt-Einmal-Garantie durch kontinuierliche Zustands-Checkpoints und Audit-Logs.

### 7. `memfuse-crypto` (Layer 1 — Zeroization & Integrity)
* **Komplexitäts-Treiber**:
  * **Kryptographisches Schlüsselmanagement**: HKDF (HMAC-based Extract-and-Expand Key Derivation Function).
  * **Anti-Tamper & Memory Safety**: Rohzeiger-Inspektion im Test zur Verifizierung von `Zeroize`-Drop-Semantiken im RAM.

### 8. `memfuse-checkpoint` (Layer 1 — Snapshot & State Isolation)
* **Komplexitäts-Treiber**:
  * **RAII Checkpoint Guards**: Deterministischer Auto-Rollback bei Fehlen von Commits.
  * **Time-Travel Consistency**: Konsistente Zustandswiederherstellung über verschiedene Schichten hinweg.

### 9. `memfuse-mcp` (Layer 4 — Model Context Protocol Server)
* **Komplexitäts-Treiber**:
  * **Protocol Adherence**: Reine stdio JSON-RPC 2.0 Implementierung (ADR-010).
  * **Sandbox Security**: AES-256-GCM-SIV Verschlüsselung für sensible Context-Outputs.

### 10. `memfuse-router` (Layer 3 — Query Routing & SLM Dispatch)
* **Komplexitäts-Treiber**:
  * **Dynamic Query Classification**: Entscheidung, ob eine Suchanfrage direkt via SLM bedient werden kann oder die volle 4-Signal RAG Pipeline erfordert.

### 11. `memfuse-embed` (Layer 3 — ONNX Cross-Encoder Reranking)
* **Komplexitäts-Treiber**:
  * **Optionales Feature Gate**: Bedingte Kompilierung (`--features onnx`) mit Passthrough-Fallback für Pure-Rust sovereign Targets.

### 12. `memfuse-py` (Layer 3 — Python PyO3 Bindings)
* **Komplexitäts-Treiber**:
  * **FFI & Async Bridge**: PyO3-Anbindungen zur sicheren Übergabe von Rust Futures an Pythons Event Loop / GIL.

### 13. `memfuse-tauri` (Layer 4 — Desktop App Backend)
* **Komplexitäts-Treiber**:
  * Inter-Process Communication (IPC) Befehle zwischen Rust-Prozess und Webview Frontend.

### 14. `memfuse-ollama` (Layer 3 — Ollama Integration)
* **Komplexitäts-Treiber**:
  * Standard Async HTTP REST-Client für die lokale Ollama-Inferenz-API.

### 15. `memfuse-core` (Layer 0 — Shared Domain Core)
* **Komplexitäts-Treiber**:
  * Geringste Komplexität: Reine Typdefinitionen (`DocId`, `TxId`, `MemoryLink`, `ContextChunk`), Traits und zentrales Fehlerhandling (`MemFuseError`).

---
*Erstellt am 2026-08-31 für MemFuse Architecture Documentation.*
