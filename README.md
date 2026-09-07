# MemFuse Brain

**Das Cognitive Operating System für lokale KI-Agenten. Air-gapped, souverän, Pure-Rust.**

MemFuse Brain ist eine Desktop-Applikation und ein eingebettetes kognitives Betriebssystem,
das Ihre Firmendokumente (PDF, Word, Markdown, E-Mails) durchsuchbar macht und
über ein lokal laufendes Sprachmodell (via Ollama) Fragen dazu beantwortet —
komplett offline, ohne dass ein einzes Byte Ihrer Daten das Gerät verlässt.

> ⚠️ **Status: Aktive Entwicklung.** Kern-Suchengine ist produktionsreif
> verifiziert (LSM-Tree, HNSW, BM25, CSR-Graph-Persistenz). Desktop-App (Tauri),
> MCP-Server und Ollama-Integration sind im Workspace vollständig integriert.

## Warum MemFuse Brain?

- **Air-Gapped by Design** — keine Cloud, keine Telemetrie, kein API-Key nötig
- **Lokal & Backend-flexibel** — läuft vollständig auf Ihrem Rechner; erfordert aktuell Ollama als LLM-/Embedding-Backend (separat zu installieren, siehe Installation unten); eine ONNX-basierte Embedding-Alternative (`memfuse-embed`) ist im Code bereits vorhanden.
- **4-Signal-Hybridsuche** — Vektorsuche (HNSW) + Volltextsuche (BM25) +
  Wissensgraph (CSR) + Metadaten-Filter, fusioniert via Reciprocal Rank Fusion (RRF)
- **Contextual Retrieval** — Automatisches Anreichern zerschnittener Chunks durch ein
  LLM-generiertes Kontext-Präfix (Anthropic Pattern) [FREMDREFERENZ: Anthropic 2024 — nicht an MemFuse validiert]
- **Cross-Encoder Reranking** — Post-RRF Neuordnung via lokalem ONNX Cross-Encoder
  (optionales Feature) [FREMDREFERENZ: Anthropic 2024 — nicht an MemFuse validiert]
- **Multi-Step Query Engine** — Iteratives Query-Rewriting für komplexe
  Agenten-Abfragen (OpenAI o-series Pattern, bis zu 3 Runden)
- **MCP Sandbox** — Sichere Tool-Isolation, Zeroize-Encryption für volatile Tool-Outputs
  (Anthropic Containment Pattern)
- **Session DAG** — Grok-Pattern: Konversationsverzweigung als persistierter,
  azyklischer Graph mit vollständiger Tauri-UI-Anbindung (Erstellen von Branches ab
  jeder Nachricht, Umschalten des aktiven Branches & Historien-Navigation)
- **Deutsche Morphologie** — versteht "Urlaubsantragsprozess" auch als
  "Urlaub", "Antrag", "Prozess" für bessere Trefferqualität
- **Verschlüsselt** — AES-256-GCM auf Disk, HMAC-Anti-Tamper im WAL

## Installation

### Systemanforderungen

- Windows 10/11, macOS 11+, oder eine gängige Linux-Distribution
- [Ollama](https://ollama.com) muss separat installiert und gestartet sein
  (MemFuse Brain nutzt Ollama als lokales LLM- & Embedding-Backend)
- Mindestens ein Ollama-Modell heruntergeladen, z.B.:
```bash
  ollama pull llama3.2
  ollama pull nomic-embed-text
```

### Aus dem Quellcode bauen

```bash
# Bauen der Tauri Desktop App
cd crates/memfuse-tauri
cargo tauri build

# Ausführen des MCP Servers
cargo run -p memfuse-mcp --bin memfuse-mcp-server -- --db-path ./firma_daten
```

## Architektur

MemFuse ist ein Workspace mit 15 Rust-Crates in 5 Layern.

```
┌───────────────────────────────────────────────────────────┐
│  MemFuse Brain (Tauri Desktop App / Layer 4)              │
│  ┌─────────────┐  ┌────────────────────┐  ┌─────────────┐ │
│  │ Chat-UI      │  │ Dokumenten-Import  │  │ MCP Server  │ │
│  └──────┬───────┘  └─────────┬──────────┘  └──────┬──────┘ │
│         │                     │                    │      │
│  ┌──────▼─────────────────────▼────────────────────▼────┐ │
│  │  memfuse-ollama (lokales LLM & Embedding Backend)     │ │
│  └──────┬───────────────────────────────────────────────┘ │
│         │                                                 │
│  ┌──────▼───────────────────────────────────────────────┐ │
│  │  memfuse-db (4-Signal RAG-Engine)                    │ │
│  │  Vektor + BM25 + Wissensgraph + Metadaten            │ │
│  └──────────────────────────────────────────────────────┘ │
└───────────────────────────────────────────────────────────┘
            Alles lokal. Nichts verlässt den Rechner.
```

### Produktionsreif ✅
| Crate | Funktion |
|---|---|
| memfuse-core | Typen, Traits, Domain-Modell |
| memfuse-crypto | WAL v3 HMAC-Chain, Kryptographie |
| memfuse-store | LSM-Storage, 16-Shard MemTable, SSTable-Compaction |
| memfuse-index | HNSW (2-Phasen-CoW-Rebuild), DiskANN (Build) |
| memfuse-text | BM25-Volltextsuche, Deutsche Morphologie |
| memfuse-embed | Embeddings (ONNX), Cross-Encoder Reranker |
| memfuse-graph | CSR-Graph, PPR, Community Detection, Session-DAG |
| memfuse-router | Conformal Router, SlmProfile-basiertes Routing |
| memfuse-db | Kernoperationen, Fusion, Search-Pipeline |
| memfuse-agent | Agent Workflow Engine |
| memfuse-ollama | Ollama Client & Embeddings |
| memfuse-mcp | MCP Server |
| memfuse-tauri | Desktop App Shell |
| memfuse-checkpoint | Backup & Snapshot Management |
| memfuse-bench | Synthetic Benchmark Harness |

### In aktiver Entwicklung ⚙️
| Crate | Status |
|---|---|
| memfuse-calibration | G0-Sprint: IsotonicCalibrator + PlattScaler |
| memfuse-kv-bridge | H2-Sprint: KV-Cache-Bridge mit Tenant-Isolation |
| memfuse-candle | H2-Sprint: Native GGUF-Inferenz (Datenhoheit) |

> **Hinweis für Entwickler:** Die verifizierte Crate-Topologie und der tatsächliche Codestand sind in `AGENTS.md` dokumentiert.
> Bei Widerspruch zwischen README und AGENTS.md: AGENTS.md hat Vorrang.

### Grounding & Quellenattribuierung (RAG Grounding)

RAG-Antworten in MemFuse Brain sind instruiert, Antworten **ausschließlich** auf Basis der im `<context>`-Block bereitgestellten Informationen zu formulieren und Fakten mit Quellenangaben im Format `[Dateiname]` oder `[Dateiname, Abschnitt]` zu belegen. Wenn eine Information nicht im Kontext enthalten ist, antwortet das Modell mit der festen Fallback-Phrase: *"Diese Information ist in den importierten Dokumenten nicht enthalten."*

> ℹ️ **Hinweis zur Modell-Sicherheit:** Die Grounding- und Zitiergebot-Instruktionen dienen als systemische Heuristik für das lokale LLM. Kleinere Sprachmodelle (z. B. 7B-Modelle wie `llama3.2`) folgen diesen Anweisungen sehr gut, können jedoch in Einzelfällen vereinzelt abweichen.

## Workspace Crates (15 Active Crates)

- **Layer 0**: `memfuse-core` (Typen, Traits, Error + ContextChunk mit Contextual Prefix)
- **Layer 1**: `memfuse-store` (LSM-Tree), `memfuse-index` (HNSW), `memfuse-text` (BM25), `memfuse-crypto` (AES-GCM), `memfuse-graph` (CSR Graph, + SessionBranchTree DAG), `memfuse-checkpoint` (Snapshotting)
- **Layer 2**: `memfuse-db` (Collections & 4-Signal Fusion, + MultiStepEngine, ContextCompactor)
- **Layer 3**: `memfuse-ollama` (Ollama Client & Embeddings, + ContextPrefixEngine, generate_text()), `memfuse-agent` (Persistent Agent Workflow Engine), `memfuse-router` (Conformal Profile Router), `memfuse-embed` (ONNX-Embeddings, **optional**, Feature-gated, `default=[]`, + CrossEncoderReranker)
- **Layer 4**: `memfuse-mcp` (MCP Server, + McpSandbox, VolatileToolResult), `memfuse-tauri` (Desktop App Shell)
- **Layer 5**: `memfuse-bench` (Reproduzierbarer Benchmark-Harness für Retrieval-Genauigkeit)

## Für Entwickler: Rust-Crates

Der Kern von MemFuse Brain ist als eigenständige, wiederverwendbare
Rust-Bibliothek verfügbar:

```toml
[dependencies]
memfuse-db = "0.1.0"
```

```rust
use memfuse_db::MemFuse;

let db = MemFuse::open("./meine_daten").await?;
let col = db.collection("dokumente").await?;

col.insert("doc-1", &embedding, Some(serde_json::json!({"text": "..."}))).await?;

let results = col.hybrid_search("meine Anfrage", &query_embedding, 5, None).await?;
```

## MCP-Server (für Claude Desktop & andere MCP-Clients)

Der `memfuse-mcp`-Server stellt MCP-Tools über stdio JSON-RPC 2.0 bereit (ADR-010) (`memfuse_search`, `memfuse_insert`, `memfuse_get`, `memfuse_collections`).

> 📖 **Vollständige MCP-Dokumentation & Konfigurationsanweisung:**
> Siehe [crates/memfuse-mcp/README.md](crates/memfuse-mcp/README.md) für Installation, Claude-Desktop-Konfiguration (`claude_desktop_config.json`), Schritt-für-Schritt Demo und Troubleshooting.

```bash
# Standardmäßig im Read-Only-Modus (Schreibzugriff gesperrt):
cargo run -p memfuse-mcp --bin memfuse-mcp-server -- --db-path ./firma_daten

# Explicit mit Schreibzugriff starten via Flag oder Env:
cargo run -p memfuse-mcp --bin memfuse-mcp-server -- --db-path ./firma_daten --allow-write
# oder:
MEMFUSE_MCP_ALLOW_WRITE=1 cargo run -p memfuse-mcp --bin memfuse-mcp-server -- --db-path ./firma_daten
```

## Roadmap — Cognitive Operating System

### ✅ Phase 1: RAG-Fundament (abgeschlossen)
- [x] LSM-Tree-Storage mit MVCC, WAL, Crash-Recovery
- [x] HNSW-Vektorindex mit SIMD-Beschleunigung
- [x] BM25-Volltextsuche mit deutscher Morphologie
- [x] CSR-Wissensgraph mit LSM-Persistenz
- [x] 4-Signal-Fusion (Vektor + BM25 + Wissensgraph + Metadaten)
- [x] Contextual Retrieval (Anthropic Pattern)
- [x] Cross-Encoder Reranking (ONNX, optional)
- [x] Multi-Step Query Engine (OpenAI o-series Pattern)
- [x] Context Compaction (Grok Pattern)
- [x] Session DAG Branching (Grok Pattern)
- [x] MCP Sandbox Isolation (Anthropic Containment)
- [x] Desktop-App (memfuse-tauri), MCP-Server, Python-Bindings

### 🔄 Phase 2: Cognitive Memory (Teilweise implementiert, Q4 2026)
- [x] Kognitive Gedächtnistypen: Episodic / Semantic / Procedural / Working Memory (`MemoryType`-Enum)
- [x] Temporaler Wissensgraph: bi-temporale Zeitachsen (Validitätszeit + Transaktionszeit)
- [x] Memory Importance Score (`ImportanceScore`, `decay_factor()`)
- [x] Recency-Decay-Funktionen (`DecayFunction`)
- [x] Aktiver Sweep-Enforcement-Loop (Reaper)
- [ ] ProvenanceRecord (abfragbarer Herkunftsnachweis pro Suchergebnis)
- [ ] Kalibriertes Kaskaden-Routing (`memfuse-router`)
- [ ] DiskANN Produktionsreife & Integration (`experimental-diskann` -> Default)

### 📋 Phase 3: Selbstorganisierung (Teilweise implementiert, Q1 2027)
- [x] Personalized PageRank (PPR) für Multi-Hop Graph-Retrieval (ADR-026)
- [x] Community Detection für semantische Cluster via Label Propagation (ADR-027)
- [x] A-MEM Zettelkasten-Pattern: Memories mit expliziten Querverweisen (ADR-038)
- [ ] Memory Consolidation: Asynchrone Sleep-Cycle-Konsolidierung via LLM
- [ ] PathRAG: Relationale Pfadextraktion
- [ ] CausalEdge: Kausale Graph-Dimension
- [ ] Verified Forgetting: Kryptographischer Löschbeweis

### 📋 Phase 4: Enterprise (Q2 2027)
- [ ] OAuth 2.0 für MCP-Server
- [ ] RBAC und Multi-Tenant-Isolation
- [ ] Immutable Audit-Trail für Compliance
- [ ] Benchmark-Suite vs. Mem0, Zep/Graphiti, MemOS

## Positionierung

MemFuse ist kein Ersatz für Cloud-Vektordatenbanken (Qdrant, Pinecone).
MemFuse is eine neue Kategorie: **Das lokale Cognitive Operating System für LLM-Agenten** — in-process, air-gapped, Pure-Rust.

| Kriterium | MemFuse | Mem0 | Zep/Graphiti | Chroma+ES+Neo4j |
|-----------|---------|------|--------------|-----------------|
| Air-gapped | ✅ | ❌ | ❌ | ✅ |
| 4-Signal Fusion | ✅ | ❌ | Teilweise | Extern |
| Pure Rust | ✅ | ❌ | ❌ | ❌ |
| MCP-nativ | ✅ | ❌ | ❌ | ❌ |
| Contextual Retrieval | ✅ | ❌ | ❌ | ❌ |
| Session DAG | ✅ | ❌ | ❌ | ❌ |
| Kein Docker | ✅ | ❌ | ❌ | ❌ |

*\*Hinweis: Alle Positionierungsclaims basieren auf den genannten Architekturmerkmalen. Zitierte Fehlerreduktions-Prozentangaben entstammen der Fachliteratur [FREMDREFERENZ: Anthropic 2024 — nicht an MemFuse validiert]. MemFuse stellt mit `benchmarks/memfuse-bench` ein eigenes Benchmark-Harness auf einem 9-Dokumenten Synthetik-Korpus bereit (Details in [`benchmarks/README.md`](benchmarks/README.md)).*

## Lizenz

MIT OR Apache-2.0
