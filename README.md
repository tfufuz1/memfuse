# MemFuse

**Souveräne, lokal betriebene Gedächtnisschicht für KI-Agenten — hochperformante, kryptographisch isolierte Embedded AI Memory Library (Python & Rust).**

MemFuse ist eine souveräne, lokal betriebene Embedded AI Memory Library für KI-Agenten — primär Python (`memfuse-py`) und Rust. Sie bietet eine hochperformante, kryptographisch isolierte Gedächtnisschicht, die Ihre Dokumente und Daten durchsuchbar macht und über ein lokal laufendes Sprachmodell (z. B. via Ollama) Fragen dazu beantwortet — komplett offline, ohne dass ein einziges Byte Ihrer Daten das Gerät verlässt.

> ⚠️ **Status: Aktive Entwicklung.** Kern-Suchengine ist produktionsreif
> verifiziert (LSM-Tree, HNSW, BM25, CSR-Graph-Persistenz). PyPI-Paket (`memfuse`)
> dient als primärer Vertriebsweg. Die Tauri-Desktop-App (`memfuse-tauri`) ist
> **deprecated** und wird am **2026-11-07** entfernt (siehe [ADR-077](DECISIONS.md#adr-077-produktvision-pypi-library-fokus-und-tauri-deprecation)).
> Bitte nutzen Sie `memfuse-py`.

## Warum MemFuse?

- **Air-Gapped by Design** — keine Cloud, keine Telemetrie, kein API-Key nötig
- **Lokal & Backend-flexibel** — läuft vollständig auf Ihrem Rechner; erfordert aktuell Ollama als LLM-/Embedding-Backend (separat zu installieren, siehe Installation unten); eine ONNX-basierte Embedding-Alternative (`memfuse-embed`) ist im Code bereits vorhanden.
- **4-Signal-Hybridsuche** — Vektorsuche (HNSW) + Volltextsuche (BM25) +
  Wissensgraph (CSR) + Metadaten-Filter, fusioniert via Reciprocal Rank Fusion (RRF)
- **Contextual Retrieval** — Automatisches Anreichern zerschnittener Chunks durch ein
  LLM-generiertes Kontext-Präfix (MemFuse Contextual Chunk Prefixing) [Referenzwert aus Fachliteratur zu Contextual-Retrieval-Verfahren — nicht am MemFuse-Korpus validiert]
- **Cross-Encoder Reranking** — Post-RRF Neuordnung via lokalem ONNX Cross-Encoder
  (optionales Feature) [Referenzwert aus Fachliteratur — nicht am MemFuse-Korpus validiert]
- **Multi-Step Query Engine** — Iteratives Query-Rewriting für komplexe
  Agenten-Abfragen (MemFuse Iterative Multi-Step Retrieval, bis zu 3 Runden)
- **MCP Sandbox** — Sichere Tool-Isolation, Zeroize-Encryption für volatile Tool-Outputs
  (MemFuse Volatile-Output Isolation)
- **Session DAG** — MemFuse Session-DAG Pattern: Konversationsverzweigung als persistierter,
  azyklischer Graph (Erstellen von Branches ab jeder Nachricht, Umschalten des aktiven Branches & Historien-Navigation)
- **Deutsche Morphologie** — versteht "Urlaubsantragsprozess" auch als
  "Urlaub", "Antrag", "Prozess" für bessere Trefferqualität
- **Verschlüsselt** — AES-256-GCM auf Disk, HMAC-Anti-Tamper im WAL

## Installation

### Als Python-Library (empfohlen)

Das PyPI-Paket `memfuse` bildet die primäre Schnittstelle für Python-Entwickler:

```bash
pip install memfuse
```

Minimales Codebeispiel:

```python
import memfuse

# Datenbank initialisieren
db = memfuse.PyMemFuse("./data")
collection = db.collection("documents")

# Dokument einfügen
collection.insert("doc_1", "MemFuse bietet hochperformante eingebettete Vektorsuche.")

# Hybridsuche ausführen
results = collection.hybrid_search("Vektorsuche")
for res in results:
    print(res.id, res.score, res.text)
```

### Für Rust-Entwickler

Der Kern von MemFuse ist als wiederverwendbare Rust-Bibliothek verfügbar:

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

### Desktop-App (deprecated, Entfernung 2026-11-07)

> ⚠️ **Deprecation-Hinweis (ADR-077):** Die Tauri-Desktop-App (`memfuse-tauri`) wird am **2026-11-07** aus dem Repository entfernt. Bitte migrieren Sie auf `memfuse-py` oder die Rust-Library `memfuse-db`.

#### Systemanforderungen

- Windows 10/11, macOS 11+, oder eine gängige Linux-Distribution
- [Ollama](https://ollama.com) muss separat installiert und gestartet sein
  (MemFuse nutzt Ollama als lokales LLM- & Embedding-Backend)
- Mindestens ein Ollama-Modell heruntergeladen, z.B.:
```bash
  ollama pull llama3.2
  ollama pull nomic-embed-text
```

#### Aus dem Quellcode bauen

```bash
# Bauen der Tauri Desktop App (deprecated)
cd crates/memfuse-tauri
cargo tauri build

# Ausführen des MCP Servers
cargo run -p memfuse-mcp --bin memfuse-mcp-server -- --db-path ./firma_daten
```

## Architektur

MemFuse ist ein Workspace mit 18 Rust-Crates in 5 Layern.

```
┌───────────────────────────────────────────────────────────┐
│  Zugangswege & Anbindungen                                │
│  ┌───────────────────────┐  ┌──────────────────────────┐  │
│  │  memfuse-py (Python)  │  │  MCP Server              │  │
│  │  (Primär / Empfohlen) │  │  (memfuse-mcp)           │  │
│  └───────────┬───────────┘  └────────────┬─────────────┘  │
│              │                           │                │
│              │   ┌───────────────────────┴──────────────┐ │
│              │   │ memfuse-tauri (Desktop App)          │ │
│              │   │ (deprecated, Entfernung 2026-11-07)  │ │
│              │   └───────────────────────┬──────────────┘ │
│              │                           │                │
│  ┌───────────▼───────────────────────────▼──────────────┐ │
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
| memfuse-py | Python FFI Bindings (PyO3) — Primärer Zugangsweg |
| memfuse-tauri | Desktop App Shell (deprecated, Entfernung 2026-11-07) |
| memfuse-checkpoint | Backup & Snapshot Management |
| memfuse-kv-bridge | KV-Cache-Bridge Sicherheitsschicht (Zeroize, Tenant-Isolation) |
| memfuse-bench | Synthetic Benchmark Harness |

### In aktiver Entwicklung ⚙️
| Crate | Status |
|---|---|
| memfuse-calibration | G0-Sprint: IsotonicCalibrator + PlattScaler |
| memfuse-candle | H2-Sprint: Native GGUF-Inferenz (Datenhoheit) |

> **Hinweis für Entwickler:** Die verifizierte Crate-Topologie und der tatsächliche Codestand sind in `AGENTS.md` dokumentiert.
> Bei Widerspruch zwischen README und AGENTS.md: AGENTS.md hat Vorrang.

### Grounding & Quellenattribuierung (RAG Grounding)

RAG-Antworten in MemFuse sind instruiert, Antworten **ausschließlich** auf Basis der im `<context>`-Block bereitgestellten Informationen zu formulieren und Fakten mit Quellenangaben im Format `[Dateiname]` oder `[Dateiname, Abschnitt]` zu belegen. Wenn eine Information nicht im Kontext enthalten ist, antwortet das Modell mit der festen Fallback-Phrase: *"Diese Information ist in den importierten Dokumenten nicht enthalten."*

> ℹ️ **Hinweis zur Modell-Sicherheit:** Die Grounding- und Zitiergebot-Instruktionen dienen als systemische Heuristik für das lokale LLM. Kleinere Sprachmodelle (z. B. 7B-Modelle wie `llama3.2`) folgen diesen Anweisungen sehr gut, können jedoch in Einzelfällen vereinzelt abweichen.

## Workspace Crates (17 Active Crates)

- **Layer 0**: `memfuse-core` (Typen, Traits, Error + ContextChunk mit Contextual Prefix)
- **Layer 1**: `memfuse-store` (LSM-Tree), `memfuse-index` (HNSW), `memfuse-text` (BM25), `memfuse-security` (AES-GCM & KV-Segment Security), `memfuse-graph` (CSR Graph, + SessionBranchTree DAG), `memfuse-checkpoint` (Snapshotting)
- **Layer 2**: `memfuse-db` (Collections & 4-Signal Fusion, + MultiStepEngine, ContextCompactor)
- **Layer 3**: `memfuse-ollama` (Ollama Client & Embeddings, + ContextPrefixEngine, generate_text()), `memfuse-agent` (Persistent Agent Workflow Engine), `memfuse-router` (Conformal Profile Router), `memfuse-embed` (ONNX-Embeddings, **optional**, Feature-gated, `default=[]`, + CrossEncoderReranker), `memfuse-py` (Python PyO3 FFI Bindings)
- **Layer 4**: `memfuse-mcp` (MCP Server, + McpSandbox, VolatileToolResult), `memfuse-tauri` (Desktop App Shell — deprecated, Entfernung 2026-11-07)
- **Layer 5**: `memfuse-bench` (Reproduzierbarer Benchmark-Harness für Retrieval-Genauigkeit)

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

## Roadmap — Embedded Agentic Memory Engine

### ✅ Phase 1: RAG-Fundament (abgeschlossen)
- [x] LSM-Tree-Storage mit MVCC, WAL, Crash-Recovery
- [x] HNSW-Vektorindex mit SIMD-Beschleunigung
- [x] BM25-Volltextsuche mit deutscher Morphologie
- [x] CSR-Wissensgraph mit LSM-Persistenz
- [x] 4-Signal-Fusion (Vektor + BM25 + Wissensgraph + Metadaten)
- [x] Contextual Retrieval (MemFuse Contextual Chunk Prefixing)
- [x] Cross-Encoder Reranking (ONNX, optional)
- [x] Multi-Step Query Engine (MemFuse Iterative Multi-Step Retrieval)
- [x] Context Compaction (MemFuse Context-Window Compaction)
- [x] Session DAG Branching (MemFuse Session-DAG Pattern)
- [x] MCP Sandbox Isolation (MemFuse Volatile-Output Isolation)
- [x] MCP-Server, Python-Bindings (`memfuse-py`)

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

### 📋 Phase 4: PyPI Release & Ecosystem Integration (Q2 2027)
- [ ] Erstes offizielles PyPI-Release (`pip install memfuse`) mit vollständigen Binar-Wheels für Linux, macOS und Windows
- [ ] PyO3 Sub-Interpreter-Isolationsprüfungen (PEP 684) & Asyncio-Loop-Bridging
- [ ] Benchmark-Suite vs. Mem0, Zep/Graphiti, MemOS

## Positionierung

MemFuse ist kein Ersatz für Cloud-Vektordatenbanken (Qdrant, Pinecone).
MemFuse ist eine neue Kategorie: **Die lokale Embedded AI Memory Library für KI-Agenten** — in-process, air-gapped, Pure-Rust.

| Kriterium | MemFuse | Mem0 | Zep/Graphiti | Chroma+ES+Neo4j |
|-----------|---------|------|--------------|-----------------|
| Air-gapped | ✅ | ❌ | ❌ | ✅ |
| 4-Signal Fusion | ✅ | ❌ | Teilweise | Extern |
| Pure Rust | ✅ | ❌ | ❌ | ❌ |
| MCP-nativ | ✅ | ❌ | ❌ | ❌ |
| Contextual Retrieval | ✅ | ❌ | ❌ | ❌ |
| Session DAG | ✅ | ❌ | ❌ | ❌ |
| Kein Docker | ✅ | ❌ | ❌ | ❌ |

*\*Hinweis: Alle Positionierungsclaims basieren auf den genannten Architekturmerkmalen. Zitierte Fehlerreduktions-Prozentangaben entstammen der Fachliteratur [Referenzwert aus Fachliteratur zu Contextual-Retrieval-Verfahren — nicht am MemFuse-Korpus validiert]. MemFuse stellt mit `benchmarks/memfuse-bench` ein eigenes Benchmark-Harness auf einem 9-Dokumenten Synthetik-Korpus bereit (Details in [`benchmarks/README.md`](benchmarks/README.md)).*

## Architektur-Entscheidungen (ADRs)

Die vollständige Begründung zur Fokussierung auf die Python/Rust-Library und der Deprecation der Desktop-Anwendung ist in [ADR-077 in DECISIONS.md](DECISIONS.md#adr-077-produktvision-pypi-library-fokus-und-tauri-deprecation) dokumentiert.

## Lizenz

MIT OR Apache-2.0
