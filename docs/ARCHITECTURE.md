# MemFuse Architektur — Kurzreferenz

## Architektur-Update: Desktop-Applikation & 13 Workspace Crates

MemFuse ist in ein 5-Schichten-Modell (Layer 0–4) gegliedert. Sämtliche 13 Crates im Workspace (12 Kern-Crates + 1 optionales Crate) halten sich an den strikten gerichteten azyklischen Graphen (DAG).

```
Layer 0:  memfuse-core        — Shared Kernel: Typen, Traits, Fehler, TextEmbeddingEngine
Layer 1:  memfuse-store       — LSM-Tree, WAL, SSTables, Crypt-at-Rest
          memfuse-index       — HNSW, SIMD-Distanz, SQ8-Quantisierung (DiskANN experimental via Feature)
          memfuse-text        — BM25, Inverted Index, Deutsche Morphologie
          memfuse-crypto      — AES-256-GCM, HMAC-Chaining
          memfuse-graph       — CSR-Graph, Entity-Relation Traversal (LSM-Persistierung unter __graph:)
          memfuse-checkpoint  — Async Checkpointing & State Snapshot Management
Layer 2:  memfuse-db          — Collections, 4-Signal Fusion (Vektor + BM25 + Graph + Metadaten), RRF, 2PC
Layer 3:  memfuse-py          — PyO3-Fassade (Python-Bindings)
          memfuse-ollama      — Ollama HTTP Client & OllamaEmbedder Provider
          memfuse-embed       — ONNX-Embeddings (optional, Feature-gated, default=[])
Layer 4:  memfuse-mcp         — Standalone MCP-Server (stdio JSON-RPC 2.0, ADR-010)
          memfuse-tauri       — Desktop-Shell ("MemFuse Brain"), IPC-Commands, Ingestion-Pipeline
```

**Aktiver Workspace-Build**: 13 Crates (`memfuse-core`, `memfuse-store`, `memfuse-index`, `memfuse-text`, `memfuse-crypto`, `memfuse-graph`, `memfuse-checkpoint`, `memfuse-db`, `memfuse-py`, `memfuse-ollama`, `memfuse-mcp`, `memfuse-tauri`, `memfuse-embed` [optional]).

## Kern-Philosophie
MemFuse ist die **eingebettete 4-Signal-Memory-Engine & RAG-Desktop-App für lokale AI-Agenten** —
air-gapped, zero-panic (angestrebt), 100% Pure-Rust Sovereign Core (mit Ollama als lokalem LLM/Embedding Backend).

## Produktstrategie
- **Hauptausrichtung**: Lokale eingebettete Agent-Memory-Library & Desktop-App "MemFuse Brain".
- **Embedding Backend**: Ollama HTTP Inferenz (`memfuse-ollama`) als primäres Embedding-Backend (ADR-008).
- **4-Signal Fusion**: Vektor + BM25 + Wissensgraph + Metadaten-Filter, kombiniert mittels Reciprocal Rank Fusion (RRF).
- **Feature**: DACH-Morphologie (German Compound Splitter) als Differenzierungsmerkmal für deutsche Sprache.
- **DiskANN**: Out-of-Core Vektorsuche ist experimentell und hinter dem Cargo-Feature `experimental-diskann` verborgen, da sie noch nicht produktionsreif in `memfuse-db` integriert ist.

## Invarianten-Status

| Invariante | Status | Befund |
|---|---|---|
| **Souveränität** (Zero-C-Deps im Core) | ✅ Erfüllt | Core-Schichten laufen in Pure Rust. Ollama übernimmt LLM/Embeddings via HTTP. |
| **Zero-Panic** | 🟡 In Arbeit | Offene `.expect()`-Stellen: `SessionPool::pop()/push()` (memfuse-embed), `snapshot.rs` (memfuse-core). Status → 🟢 wenn `grep -rn '.expect(' crates/*/src/` null ergibt (exkl. tests). |
| **Determinismus** (SIMD) | ✅ Erfüllt | Cross-Check SIMD vs. Skalar via Proptest. |
| **WAL-Crash-Consistency** | ✅ Erfüllt | Fault-Injection im WAL, HMAC-Chaining. |
| **Graph-Persistenz** | ✅ Erfüllt | Persistierung im LSM-Tree unter den Präfixen `__graph:entity:` und `__graph:edge:`. |
| **DAG Integrity** | ✅ Erfüllt | Unidirektionale Schichten-Abhängigkeiten von Layer 0 bis Layer 4. |
| **Disk-I/O Isolation** | ✅ Erfüllt | tokio::fs für Metadaten/Lifecycle, std::fs::File ausschließlich innerhalb spawn_blocking für Block-Level Random-Access (ADR-012). |

## Sicherheit & Privacy
- **HKDF Key Derivation**: Kryptographischer Kontext pro Datei.
- **HMAC Chaining**: WAL-Integrität gegen Manipulation geschützt.
- **Namespace Isolation**: Vollständige Trennung von Collections auf Storage-Ebene.
