# MemFuse Architektur — Kurzreferenz

## Architektur-Update: Desktop-Applikation & 13 Workspace Crates

MemFuse ist in ein 5-Schichten-Modell (Layer 0–4) gegliedert. Sämtliche 13 Crates im Workspace (12 Kern-Crates + 1 optionales Crate) halten sich an den strikten gerichteten azyklischen Graphen (DAG).

```
Layer 0:  memfuse-core        — Shared Kernel: Typen, Traits, Fehler, TextEmbeddingEngine
Layer 1:  memfuse-store       — LSM-Tree, WAL, SSTables, Crypt-at-Rest
          memfuse-index       — HNSW, SIMD-Distanz, SQ8-Quantisierung (DiskANN experimental via Feature)
          memfuse-text        — BM25, Inverted Index, Deutsche Morphologie
          memfuse-crypto      — AES-256-GCM, HMAC-Chaining
          memfuse-graph       — CSR-Graph, Entity-Relation Traversal, Session DAG (SessionBranchTree)
          memfuse-checkpoint  — Async Checkpointing & State Snapshot Management
Layer 2:  memfuse-db          — Collections, 4-Signal Fusion, RRF, Multi-Step Engine, Context Compactor
Layer 3:  memfuse-py          — PyO3-Fassade (Python-Bindings)
          memfuse-ollama      — Ollama HTTP Client, OllamaEmbedder, ContextPrefixEngine
          memfuse-embed       — ONNX-Embeddings & CrossEncoderReranker (optional, Feature-gated, default=[])
Layer 4:  memfuse-mcp         — Standalone MCP-Server (stdio JSON-RPC 2.0, ADR-010), McpSandbox
          memfuse-tauri       — Desktop-Shell ("MemFuse Brain"), IPC-Commands, Ingestion-Pipeline
```

**Aktiver Workspace-Build**: 13 Crates (`memfuse-core`, `memfuse-store`, `memfuse-index`, `memfuse-text`, `memfuse-crypto`, `memfuse-graph`, `memfuse-checkpoint`, `memfuse-db`, `memfuse-py`, `memfuse-ollama`, `memfuse-mcp`, `memfuse-tauri`, `memfuse-embed` [optional]).

## Kern-Philosophie
MemFuse ist das **Cognitive Operating System für LLM-Agenten — 4-Signal-RAG-Engine mit Contextual Retrieval, Cross-Encoder Reranking, Multi-Step Query, Session DAG und MCP Sandbox** — air-gapped, zero-panic (angestrebt), 100% Pure-Rust Sovereign Core (mit Ollama als lokalem LLM/Embedding Backend).

## RAG-Pipeline (Phase 1, abgeschlossen)

MemFuse implementiert eine gestaffelte, mehrstufige Retrieval- und Ingestion-Pipeline (ADR-021):

1. **Contextual Ingestion**: `ContextPrefixEngine` (`memfuse-ollama`) generiert 50–100 Token Kontext-Präfixe vor der BM25- und Embedding-Indexierung.
2. **4-Signal Hybrid-Indexierung**: Parallele Indexierung von HNSW-Vektoren, BM25-Volltext (mit Kontext-Präfix), CSR-Wissensgraph und Metadaten-Filtern.
3. **Hybrid Retrieval via RRF**: Fusion aller Signale über `reciprocal_rank_fusion()` in `memfuse-db`.
4. **Multi-Step Query Expansion**: Iteratives Abfrage-Rewriting via `MultiStepEngine` (`memfuse-db`) für komplexe Abfragen (bis zu 3 Runden).
5. **Cross-Encoder Reranking**: Post-RRF Neuordnung der Top-K Treffer via `CrossEncoderReranker` in `memfuse-embed` (optional via `--features onnx`, Passthrough-Fallback ohne ONNX).
6. **Context Compaction**: Komprimierung langer Agenten-Historien via `ContextCompactor` (`memfuse-db`) durch Ersetzung veralteter Tool-Outputs mit `StatusToken`.

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
- **MCP Sandbox Containment**: Tool-Outputs in `memfuse-mcp` via AES-256-GCM-SIV verschlüsselt und Zeroize beim Verwerfen.

## Planungsdokument-Blueprint-Korrekturen
Standard-Korrekturen für Architektur- und Planungs-Blueprints:
1. **`SessionPool` Sichtbarkeit**: `SessionPool` (bzw. `OnnxSessionPool`) ist `pub(crate)` in `memfuse-embed` und nicht exportiert. Für Modul-externe Nutzung bzw. Cross-Encoder (wie in `reranker.rs`) wird ein eigener `SessionPool` gehalten.
2. **Keine `petgraph`-Abhängigkeit**: `memfuse-graph` nutzt eine native Pure-Rust CSR-Graph-Implementierung (`SessionBranchTree`, `CsrGraph`) ohne `petgraph`-Workspace-Abhängigkeit gemäß ADR-004 (Pure Rust Sovereign Core Policy).
3. **`CheckpointGuard` RAII & Snapshot-Referenzen**: `CheckpointGuard` besitzt RAII-Semantik (Auto-Rollback bei Drop) und ist bewusst nicht klonbar. Zustands-Referenzen werden als `snapshot_tx_id: Option<TxId>` gespeichert.
4. **RRF-Nutzung**: `memfuse-db::fusion` stellt `reciprocal_rank_fusion()` und `weighted_reciprocal_rank_fusion()` bereit. Spezifikationen und Blueprints nutzen bestehende Funktionen anstelle redundanter `execute_rrf()` Neuimplementierungen.
