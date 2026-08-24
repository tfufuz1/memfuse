# MemFuse — Source of Truth (SOT)

> **Dieses Dokument ist das einzige Living State Document für Architektur-Status, Crate-Inventar, offene Findings und die aktive Roadmap. Es wird synchron mit dem Code aktualisiert — niemals im Voraus.**

---

## Aktueller Projektstatus (Stand: MemFuse Brain & 4-Signal Fusion)

**Produkt**: MemFuse Brain — lokale, air-gapped RAG-Desktop-App & 4-Signal Memory Engine für lokale AI-Agenten
**Kern-USP**: Echtes 4-Signal-Hybrid-RAG (Vektor + BM25 + Wissensgraph + Metadaten-Filter), persistiert,
in `hybrid_search()` fusioniert — verifizierter Code-Zustand.

### Aktive Workspace Crates (12 Kern-Crates + 1 optionales Crate)
- `memfuse-core` (Layer 0) — Typen, Primitiven, Fehler, Shared Traits (`TextEmbeddingEngine`)
- `memfuse-store` (Layer 1) — LSM-Tree, WAL, SSTables, Crypt-at-Rest
- `memfuse-index` (Layer 1) — HNSW-Vektorindex, SIMD-Distanzen, SQ8-Quantisierung
- `memfuse-text` (Layer 1) — BM25 Inverted Index, Deutsche Morphologie
- `memfuse-crypto` (Layer 1) — AES-256-GCM, HMAC-Chaining
- `memfuse-graph` (Layer 1) — CSR-Graph, persistent im LSM-Tree (`__graph:entity:` & `__graph:edge:`)
- `memfuse-checkpoint` (Layer 1) — Async Checkpointing & State Snapshot Management
- `memfuse-db` (Layer 2) — Collections Orchestrator, 4-Signal-Fusion, RRF, Transaktionalität
- `memfuse-py` (Layer 3) — PyO3 Python-Bindings, CRUD & Hybrid-Suche
- `memfuse-ollama` (Layer 3) — Ollama HTTP Client & `OllamaEmbedder` für Vektor-Embeddings
- `memfuse-mcp` (Layer 4) — Standalone MCP-Server (stdio JSON-RPC 2.0, ADR-010) mit Ollama-Embedding-Integration (`memfuse_search`, `memfuse_insert`, `memfuse_get`, `memfuse_collections`)
- `memfuse-embed` (Layer 3, **optional**) — ONNX-Embeddings, Feature-gated (`default = []`). Bewusste Randstellung — Pure-Rust-USP durch `default=[]` gewahrt. Wird bei Bedarf mit `--features onnx` aktiviert.
- `memfuse-tauri` (Layer 4) — MemFuse Brain Desktop-App (Tauri IPC, Ingestion-Pipeline, Chat-UI, Ollama-Diagnose)

### System-Setup & Status
- **Embedding-Backend**: Ollama via `memfuse-ollama` ist primäres Backend (ADR-008). `memfuse-embed` (ONNX) ist optional verfügbar.
- **Graph-Persistenz**: CSR-Graph wird über LSM-Store unter den Präfixen `__graph:entity:` und `__graph:edge:` vollständig persistiert.
- **MCP-Server**: stdio JSON-RPC 2.0 Transport (ADR-010). Unterstützt Volltext-, Hybrid-Suche, Dokumenten-Retrieval und automatisches Embedding beim Einfügen über `memfuse-ollama`.

---

## 1. Produktstrategie & Mission

**MemFuse** ist die **eingebettete 4-in-1 Memory Engine & RAG-Desktop-App für Lokale AI-Agenten** — kombiniert Vektorsuche (semantisch), BM25-Volltextsuche (lexikalisch), Entity-Relation Graph Traversal (assoziativ) und Metadaten-Filterung in einer in-process Bibliothek und Desktop-Anwendung.

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
          memfuse-graph       — CSR-Graph, Entity-Relation Traversal (LSM-Persistenz)
          memfuse-checkpoint  — Async Checkpointing & State Snapshot Management
Layer 2:  memfuse-db          — Collections, 4-Signal-Fusion, RRF, transaktionales 2PC
Layer 3:  memfuse-py          — PyO3 Python FFI-Bindings
          memfuse-ollama      — Ollama Client & Embedder Provider
Layer 3:  memfuse-embed       — ONNX Embeddings (optional, Feature-gated, default=[])
Layer 4:  memfuse-mcp         — Model Context Protocol (MCP) stdio JSON-RPC 2.0 Server (ADR-010)
          memfuse-tauri       — Desktop Application Shell ("MemFuse Brain")
```

---

## 3. Crate-Inventar & Status (12 Kern-Crates + 1 optionales Crate)

| Crate | Layer | LOC | Status | Beschreibung / Hauptaufgabe |
| :--- | :---: | :---: | :--- | :--- |
| `memfuse-core` | 0 | ~1.150 | 🟢 Clean | Shared Kernel, Typen, Fehler und TextEmbeddingEngine Trait. |
| `memfuse-store` | 1 | ~4.130 | 🟢 Upgraded | LSM-Tree-Storage, WAL, SSTables. |
| `memfuse-index` | 1 | ~3.520 | 🟢 Upgraded | HNSW-Vektorindex mit SIMD-Beschleunigung. |
| `memfuse-text` | 1 | ~960 | 🟢 Clean | BM25 Inverted Index & Deutsche Morphologie. |
| `memfuse-crypto`| 1 | ~310 | 🟢 Clean | Cryptographic Primitives, AES-256-GCM. |
| `memfuse-graph` | 1 | ~520 | 🟢 Active | CSR Graph mit LSM-Persistenz (`__graph:`). |
| `memfuse-checkpoint`| 1 | ~600 | 🟢 Clean | Async Checkpointing & Snapshot Management. |
| `memfuse-db` | 2 | ~2.500 | 🟢 Active | Collections Orchestrator, 4-Signal-Fusion. |
| `memfuse-py` | 3 | ~1.000 | 🟢 Active | PyO3-Fassade für Python. |
| `memfuse-ollama` | 3 | ~400 | 🟢 Active | Ollama HTTP Embedding Client & Model Info. |
| `memfuse-mcp` | 4 | ~350 | 🟢 Active | MCP Server (stdio JSON-RPC 2.0, ADR-010) für Tool Calls. |
| `memfuse-tauri` | 4 | ~2.100 | 🟢 Active | Tauri Desktop App Shell ("MemFuse Brain"), Ingestion Pipeline. |
| `memfuse-embed` | 3 | ~400 | 🧊 Optional | ONNX-Embeddings, Feature-gated (`default=[]`). Pure-Rust-USP gewahrt. |

---

## 4. Qualitäts-Gates & Definition of Done

* **Automatisierter Gate-Stack**:
  1. `cargo check --workspace`: Typsystem und Workspace-Kompilierbarkeit.
  2. `cargo test --workspace`: Gesamte Testsuite ausführen.
  3. `just check`: Formatierung und Clippy-Warnungen als Fehler behandeln.
  4. `just triple-test`: Führt cargo test 3x hintereinander aus (Flaky-Test-Detektor).

* **Invarianten-Status**:
  - **Zero-Panic**: 🟡 In Arbeit — offene `.expect()`-Stellen: `SessionPool::pop()`/`push()` in memfuse-embed (3 Stellen), `snapshot.rs` in memfuse-core (2 Stellen). Status wird auf 🟢 gesetzt wenn: `grep -rn '.expect(' crates/*/src/ --include='*.rs'` null ergibt (exkl. `#[cfg(test)]`).

* **CI-Verifikations-Prinzip**: Statusindikatoren (🟢/🟡/🔴) werden AUSSCHLIESSLICH durch CI-Ergebnisse gesetzt, niemals manuell durch Agenten-Einschätzung.
