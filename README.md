# MemFuse Brain

**Das Cognitive Operating System für lokale KI-Agenten. Air-gapped, souverän, Pure-Rust.**

MemFuse Brain ist eine Desktop-Applikation und ein eingebettetes kognitives Betriebssystem,
das Ihre Firmendokumente (PDF, Word, Markdown, E-Mails) durchsuchbar macht und
über ein lokal laufendes Sprachmodell (via Ollama) Fragen dazu beantwortet —
komplett offline, ohne dass ein einziges Byte Ihrer Daten das Gerät verlässt.

> ⚠️ **Status: Aktive Entwicklung.** Kern-Suchengine ist produktionsreif
> verifiziert (LSM-Tree, HNSW, BM25, CSR-Graph-Persistenz). Desktop-App (Tauri),
> MCP-Server und Ollama-Integration sind im Workspace vollständig integriert.

## Warum MemFuse Brain?

- **Air-Gapped by Design** — keine Cloud, keine Telemetrie, kein API-Key nötig
- **Lokal & Backend-flexibel** — läuft vollständig auf Ihrem Rechner; erfordert aktuell Ollama als LLM-/Embedding-Backend (separat zu installieren, siehe Installation unten); eine ONNX-basierte Embedding-Alternative (`memfuse-embed`) ist im Code bereits vorhanden.
- **4-Signal-Hybridsuche** — Vektorsuche (HNSW) + Volltextsuche (BM25) +
  Wissensgraph (CSR) + Metadaten-Filter, fusioniert via Reciprocal Rank Fusion (RRF)
- **Contextual Retrieval** — Automatisches Anreichern zerschnittener Chunks durch ein
  LLM-generiertes Kontext-Präfix (Anthropic Pattern, 49% weniger Retrieval-Fehler)*
- **Cross-Encoder Reranking** — Post-RRF Neuordnung via lokalem ONNX Cross-Encoder
  (optionales Feature, 67% weniger Fehler kombiniert)*
- **Multi-Step Query Engine** — Iteratives Query-Rewriting für komplexe
  Agenten-Abfragen (OpenAI o-series Pattern, bis zu 3 Runden)
- **MCP Sandbox** — Sichere Tool-Isolation, Zeroize-Encryption für volatile Tool-Outputs
  (Anthropic Containment Pattern)
- **Session DAG** — Grok-Pattern: Konversationsverzweigung als persistierter,
  azyklischer Graph (Native Pure-Rust)
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

MemFuse implementiert eine mehrstufige RAG-Pipeline:
Contextual Ingestion → 4-Signal Hybrid Index → Multi-Step Retrieval → Cross-Encoder Reranking → Context Compaction

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

## Workspace Crates (15 Active Crates)

- **Layer 0**: `memfuse-core` (Typen, Traits, Error + ContextChunk mit Contextual Prefix)
- **Layer 1**: `memfuse-store` (LSM-Tree), `memfuse-index` (HNSW), `memfuse-text` (BM25), `memfuse-crypto` (AES-GCM), `memfuse-graph` (CSR Graph, + SessionBranchTree DAG), `memfuse-checkpoint` (Snapshotting)
- **Layer 2**: `memfuse-db` (Collections & 4-Signal Fusion, + MultiStepEngine, ContextCompactor)
- **Layer 3**: `memfuse-py` (Python PyO3 Bindings), `memfuse-ollama` (Ollama Client & Embeddings, + ContextPrefixEngine, generate_text()), `memfuse-agent` (Persistent Agent Workflow Engine), `memfuse-embed` (ONNX-Embeddings, **optional**, Feature-gated, `default=[]`, + CrossEncoderReranker)
- **Layer 4**: `memfuse-mcp` (MCP Server, + McpSandbox, VolatileToolResult), `memfuse-tauri` (Desktop App Shell)

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

Der `memfuse-mcp`-Server stellt MCP-Tools über stdio JSON-RPC 2.0 bereit (ADR-010) (`memfuse_search`, `memfuse_insert`, `memfuse_get`, `memfuse_collections`):

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
MemFuse ist eine neue Kategorie: **Das lokale Cognitive Operating System für LLM-Agenten** — in-process, air-gapped, Pure-Rust.

| Kriterium | MemFuse | Mem0 | Zep/Graphiti | Chroma+ES+Neo4j |
|-----------|---------|------|--------------|-----------------|
| Air-gapped | ✅ | ❌ | ❌ | ✅ |
| 4-Signal Fusion | ✅ | ❌ | Teilweise | Extern |
| Pure Rust | ✅ | ❌ | ❌ | ❌ |
| MCP-nativ | ✅ | ❌ | ❌ | ❌ |
| Contextual Retrieval | ✅ | ❌ | ❌ | ❌ |
| Session DAG | ✅ | ❌ | ❌ | ❌ |
| Kein Docker | ✅ | ❌ | ❌ | ❌ |

*\*Hinweis: Alle Positionierungsclaims und Fehlerreduktions-Prozentangaben (Anthropic Pattern) sind fremdreferenzierte Forschungswerte bzw. architektonisch begründet, empirisch an MemFuse selbst jedoch noch nicht validiert.*

## Lizenz

MIT OR Apache-2.0
