# MemFuse — Source of Truth (SOT)

> **Dieses Dokument ist das einzige Living State Document für Produktstrategie, Roadmap und Entscheidungskontext. Es wird synchron mit dem Code aktualisiert — niemals im Voraus.**

---

## Dokumenten-Landkarte

| Datei | Zuständigkeit | Quelle |
|---|---|---|
| `AGENTS.md` | Verbindliche Verhaltensregeln für Agenten (was tun/nicht tun) | Manuell, stabil, selten geändert |
| `docs/ARCHITECTURE.md` | Technische Ist-Architektur: DAG, Layer, Crate-Zweck | **Automatisch generiert** aus Cargo.toml + Tags |
| `docs/SOURCE_OF_TRUTH.md` | Produktstrategie, Roadmap, Entscheidungskontext (WARUM) | Manuell (Strategie) + automatischer Inventar-Abschnitt |
| `WORKING_STATE.md` | Nur Session-zu-Session-Handoff: was ist gerade offen | **Automatisch generiert** aus Tags, minimal manueller Zusatz |
| `DECISIONS.md` | ADR-Log, chronologisch, append-only | Manuell |
| `docs/TYPE_REGISTRY.md` | Zentrales Typ- und Trait-Register (Kollisionsschutz) | Manuell + xtask-referenziert |
| `.jules/AUDIT_INTAKE_PROTOCOL.md` | Protokoll zur Verifikation externer Audit-Befunde | Manuell |
| `rules/*.md` | Domänenspezifische Detailregeln (SIMD, Crypto, Testing) | Manuell |

---

## Aktueller Projektstatus (Stand: MemFuse Brain & Cognitive OS)

**Produkt**: MemFuse Brain — Cognitive Operating System für lokale KI-Agenten. Air-gapped RAG-Desktop-App, 4-Signal Memory Engine, Contextual Retrieval, Cross-Encoder Reranking, MCP Sandbox — souverän, Pure-Rust, kein Docker.
**Kern-USP**: Echtes 4-Signal-Hybrid-RAG (Vektor + BM25 + Wissensgraph + Metadaten-Filter), persistiert,
in `hybrid_search()` fusioniert — verifizierter Code-Zustand.

### Workspace Topology & Inventar
Die technische Ist-Architektur, DAG-Topologie sowie Layer-Aufteilung der Workspace-Crates sind in [`docs/ARCHITECTURE.md`](ARCHITECTURE.md) definiert und werden dort via `xtask sync-docs` automatisch generiert.

### System-Setup & Status
- **Snapshot Isolation**: 🟢 Vollständig (Storage Engine via `scan_prefix_at`, Text Engine via BM25 `search_at`, Vektor Engine via HNSW `search_at` & `SequenceLog`).
- **Embedding-Backend**: Ollama via `memfuse-ollama` ist primäres Backend (ADR-008). `memfuse-embed` (ONNX) ist optional verfügbar.
- **Graph-Persistenz**: CSR-Graph wird über LSM-Store unter den Präfixen `__graph:entity:` und `__graph:edge:` vollständig persistiert.
- **MCP-Server**: stdio JSON-RPC 2.0 Transport (ADR-010). Unterstützt Volltext-, Hybrid-Suche, Dokumenten-Retrieval und automatisches Embedding beim Einfügen über `memfuse-ollama`.
- **Contextual Retrieval**: `ContextPrefixEngine` via `memfuse-ollama`. Erfordert laufende Ollama-Instanz für Präfix-Generierung. Aktivierung: `contextual_prefix` in `ContextChunk` setzen.
- **Reranking**: `CrossEncoderReranker` in `memfuse-embed` (`--features onnx`). Erfordert `bge-reranker-base.onnx` + `tokenizer.json` in `models/`. Passthrough ohne ONNX (keine Verschlechterung, nur kein Reranking).
- **MCP-Sandbox**: `McpSandbox` automatisch aktiv in `memfuse-mcp`. `SandboxPolicy`: DB-Reads erlaubt, DB-Writes und Code-Execution opt-in. Tool-Outputs AES-256-GCM-SIV verschlüsselt, Zeroize bei Drop.

---

## 1. Produktstrategie & Mission

**MemFuse** ist das **Cognitive Operating System für lokale KI-Agenten** — kombiniert Vektorsuche (semantisch), BM25-Volltextsuche (lexikalisch), Entity-Relation Graph Traversal (assoziativ) und Metadaten-Filterung in einer in-process Bibliothek und Desktop-Anwendung.

### 🎯 Kern-USP (Der 4-in-1 Vorteil)
* **Keine Ops-Last**: Air-gapped Desktop App & In-Process Library, zero Server, zero Docker.
* **4-Signal-Fusion (RRF)**: Vektor + Volltext + Graph + Metadaten-Filter vereint in einer einzigen Abfrage für optimalen LLM-Prompt-Kontext.
* **Sovereign Core**: 100% Pure-Rust Core ohne C-Abhängigkeiten (Ollama HTTP Integration for Local LLM/Embeddings).
* **ACID-Garantie**: Transaktionssicherheit durch MVCC-Snapshot-Isolation und HMAC-chained WAL.

---

## 2. Architektur-Topologie (DAG)

```
Layer 0:  memfuse-core        — Typen, Primitiven, Fehler, Embedding Trait (keine Abhängigkeiten)
Layer 1:  memfuse-store       — LSM-Tree, WAL, SSTables, Crypt-at-Rest
          memfuse-index       — HNSW, SIMD-Distanzen, SQ8-Quantisierung
          memfuse-text        — BM25, Inverted Index, Deutsche Morphologie
          memfuse-crypto      — AES-256-GCM, HMAC-Chaining
          memfuse-graph       — CSR-Graph, Entity-Relation Traversal (LSM-Persistenz), Session DAG
          memfuse-checkpoint  — Async Checkpointing & State Snapshot Management
Layer 2:  memfuse-db          — Collections, 4-Signal-Fusion, RRF, Multi-Step Engine, Context Compactor
Layer 3:  memfuse-py          — PyO3 Python FFI-Bindings
          memfuse-ollama      — Ollama Client & Embedder Provider, ContextPrefixEngine
Layer 3:  memfuse-embed       — ONNX Embeddings & CrossEncoderReranker (optional, Feature-gated, default=[])
Layer 4:  memfuse-mcp         — Model Context Protocol (MCP) stdio JSON-RPC 2.0 Server, McpSandbox (ADR-010)
          memfuse-tauri       — Desktop Application Shell ("MemFuse Brain")
```

---

## 3. Crate-Inventar & Status (13 Kern-Crates + 1 optionales Crate)

<!-- AUTOGENERATED:START:CRATE_INVENTORY -->
| Crate | Layer | LOC | Status | Beschreibung / Hauptaufgabe |
| :--- | :---: | :---: | :--- | :--- |
| `memfuse-core` | 0 | 9205 | 🟢 Clean | Core types, traits, and error handling for MemFuse |
| `memfuse-crypto` | 0 | 2422 | 🟢 Clean | Encryption at Rest utilities for MemFuse |
| `memfuse-checkpoint` | 1 | 5130 | 🟢 Clean | Backup and snapshot management for MemFuse storage |
| `memfuse-embed` | 1 | 1327 | 🧊 Optional |  |
| `memfuse-graph` | 1 | 7292 | 🟢 Clean | CSR-Graph for entity-relation traversal (Signal 3 in 4-Signal Fusion) |
| `memfuse-index` | 1 | 11900 | 🟢 Clean | HNSW vector index with SIMD distance computation for MemFuse |
| `memfuse-store` | 1 | 15245 | 🟢 Clean | LSM-Tree storage engine for MemFuse |
| `memfuse-text` | 1 | 5160 | 🟢 Clean | MemFuse — Text processing and BM25 search for Hybrid Search |
| `memfuse-db` | 2 | 19867 | 🟢 Clean | MemFuse — Embedded hybrid-search for AI agents |
| `memfuse-agent` | 3 | 5453 | 🟢 Clean | Persistent agent workflow engine for MemFuse — checkpoint/execute/audit loop |
| `memfuse-ollama` | 3 | 3341 | 🟢 Clean |  |
| `memfuse-py` | 3 | 1500 | 🟢 Clean | Python bindings for MemFuse using PyO3 |
| `memfuse-router` | 3 | 3335 | 🟢 Clean |  |
| `memfuse-mcp` | 4 | 3678 | 🟢 Clean |  |
| `memfuse-tauri` | 4 | 5170 | 🟢 Clean |  |
<!-- AUTOGENERATED:END:CRATE_INVENTORY -->

---

## 4. Qualitäts-Gates & Definition of Done

* **Automatisierter Gate-Stack**:
  1. `cargo check --workspace`: Typsystem und Workspace-Kompilierbarkeit.
  2. `cargo test --workspace`: Gesamte Testsuite ausführen.
  3. `just check`: Formatierung und Clippy-Warnungen als Fehler behandeln.
  4. `just triple-test`: Führt cargo test 3x hintereinander aus (Flaky-Test-Detektor).

* **Invarianten-Status**:
  - **Zero-Panic**: 🟢 Vollständig. Alle verbleibenden `.expect()`-Stellen im Produktionscode wurden auf Fehlerbehandlung umgestellt oder entfernt.

* **CI-Verifikations-Prinzip**: Statusindikatoren (🟢/🟡/🔴) werden AUSSCHLIESSLICH durch CI-Ergebnisse gesetzt, niemals manuell durch Agenten-Einschätzung.

---

## Aktive Sprint-Roadmap

### Phase 1 — RAG-Fundament (✅ Abgeschlossen)
Sprint RAG-01: Contextual Retrieval (ADR-019) ✅
Sprint RAG-02: Cross-Encoder Reranking ✅
Sprint RAG-03: Multi-Step Query Engine ✅
Sprint RAG-04: Context Compaction ✅
Sprint RAG-05: Session DAG + MCP Sandbox ✅

### Phase 1.5 — Härtung & Stabilität (🔄 Laufend)
- ✅ TOMBSTONE_BIT-Maskierung in `rollback_to_tx` (ADR-041)
- ✅ Atomare SSTable-Umbenennung in Compaction
- ✅ Flush-Race-Window behoben (ADR-043)
- ✅ DAG-Verletzung Router→MCP behoben (ADR-045)
- ✅ Collection.rs Modularisierung (ADR-040)
- ✅ `TxBuffer` Kapazitätsgrenze (AGT-CORE-001)
- ✅ `MemFuseErrorDto` FFI-Fehlertypen
- 🔲 `overflow-checks = true` im Release-Profil
- ✅ Aktiver Decay-/TTL-Sweep (Enforcement-Loop für Reaper)

### Phase 2 — Cognitive Memory (🔄 Teilweise implementiert, Q4 2026)
Grundbausteine implementiert:
- ✅ Kognitive Gedächtnistypen: `MemoryType`-Enum (Episodic/Semantic/Procedural/Working) mit `default_decay()`, `default_ttl_tx()` (ADR-025)
- ✅ Bi-temporale Graph-Kanten: `valid_from`/`valid_to` via TxId (ADR-033)
- ✅ Memory Importance Scoring: `ImportanceScore`, `decay_factor()` (ADR-025)
- ✅ Recency Decay: `DecayFunction::Exponential`/`StepFloor`/`None`
- ✅ A-MEM Zettelkasten Memory Links: `ContextChunk.links` mit `LinkRelation` (ADR-038)

Noch offen:
- 🔲 ProvenanceRecord (abfragbarer Herkunftsnachweis pro Suchergebnis)
- 🔲 Kalibriertes Kaskaden-Routing in `memfuse-router` (statt Score-Aggregation)
- 🔲 DiskANN-Reifung: Feature-Flag → Produktionsintegration in Collection (ADR-037)
- 🔲 Benchmark-Suite vs. Mem0/Zep/MemOS

### Phase 3 — Selbstorganisierung (🔄 Teilweise implementiert, Q1 2027)
Bereits implementiert:
- ✅ Personalized PageRank (PPR) — Power-Iteration mit L1-Norm-Abbruch (ADR-026)
- ✅ Community Detection — Label Propagation, deterministisch (ADR-027)

Noch offen:
- 🔲 Memory Consolidation & Reflection (Sleep-Cycle-Konsolidierung)
- 🔲 PathRAG: `GraphTraversalStrategy::PathExtraction`
- 🔲 CausalEdge (4. Graph-Dimension)
- 🔲 Verified Forgetting (kryptographischer Löschbeweis)

### Phase 4 — Enterprise (📋 Geplant, Q2 2027)
- 🔲 OAuth 2.0, RBAC, Multi-Tenant, Audit-Trail
