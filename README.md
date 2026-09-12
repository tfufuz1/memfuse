# MemFuse

**Souveräne, lokal betriebene Gedächtnisschicht für KI-Agenten — hochperformante, kryptographisch isolierte Embedded AI Memory Library (Python & Rust).**

> ⚡ **5-Minuten Quickstart** *(Zielzustand — vorausgesetzt Merge von PR #A-2/#A-3/#A-4)*

```bash
# Python Library installieren:
pip install memfuse

# ODER MCP-Server via uvx starten:
uvx memfuse-mcp --db-path ./data
```

```python
import memfuse

# Datenbank & Collection initialisieren
db = memfuse.PyMemFuse("./data")
collection = db.collection("documents")

# Dokument einfügen & Hybridsuche ausführen
collection.insert("doc_1", "MemFuse bietet hochperformante eingebettete Vektorsuche.")
results = collection.hybrid_search("Vektorsuche")
for res in results:
    print(res.id, res.score, res.text)
```

📖 **Ausführliche Architektur- & System-Dokumentation:** Siehe [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)

---

> ⚠️ **Status: Aktive Entwicklung.** Kern-Suchengine ist produktionsreif
> verifiziert (LSM-Tree, HNSW, BM25, CSR-Graph-Persistenz). PyPI-Paket (`memfuse`)
> dient als primärer Vertriebsweg. Die Tauri-Desktop-App (`memfuse-tauri`) ist
> **deprecated** und wird am **2026-11-07** entfernt (siehe [ADR-077](DECISIONS.md#adr-077-produktvision-pypi-library-fokus-und-tauri-deprecation)).
> Bitte nutzen Sie `memfuse-py`.

## Warum MemFuse?

- **Air-Gapped-fähig** — keine Cloud, keine Telemetrie, kein API-Key nötig (erfordert lokales LLM-/Embedding-Backend, standardmäßig Ollama)
- **Lokal & Backend-flexibel** — läuft vollständig auf Ihrem Rechner; erfordert aktuell Ollama als LLM-/Embedding-Backend (separat zu installieren); eine ONNX-basierte Embedding-Alternative (`memfuse-embed`) ist im Code vorhanden.
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

## Integrationen & Zugangswege

### Als Python-Library

Für Python-Entwickler bietet `memfuse` (PyO3 Bindings) direkte In-Process Performance:

```bash
pip install memfuse
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

### MCP-Server (für Claude Desktop & AI Agents)

Der `memfuse-mcp`-Server stellt MCP-Tools über stdio JSON-RPC 2.0 bereit (ADR-010) (`memfuse_search`, `memfuse_insert`, `memfuse_get`, `memfuse_collections`).

> 📖 **Vollständige MCP-Dokumentation & Konfigurationsanweisung:**
> Siehe [crates/memfuse-mcp/README.md](crates/memfuse-mcp/README.md) für Installation, Claude-Desktop-Konfiguration (`claude_desktop_config.json`), Schritt-für-Schritt Demo und Troubleshooting.

```bash
# MCP-Server via Cargo starten:
cargo run -p memfuse-mcp --bin memfuse-mcp-server -- --db-path ./firma_daten
```

### Systemanforderungen & Voraussetzungen

- Windows 10/11, macOS 11+, oder eine gängige Linux-Distribution
- [Ollama](https://ollama.com) separat installiert und gestartet (MemFuse nutzt Ollama als lokales LLM- & Embedding-Backend)
- Mindestens ein Ollama-Modell heruntergeladen, z.B.:
```bash
ollama pull llama3.2
ollama pull nomic-embed-text
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

## Benchmarks & Evaluierung

MemFuse nutzt ein reproduzierbares Benchmark-Harness in `benchmarks/memfuse-bench` zur kontinuierlichen Validierung der Retrieval-Qualität. In den CI-Pipelines (`.github/workflows/bench.yml`) wird `longmemeval_s` (aus `xiaowu0162/longmemeval`) zusammen mit `locomo` als automatisierte Qualitätsschranke (Regression Gate) verwendet.

Die Begrenzung der CI-Baseline auf `longmemeval_s` (anstelle von `_m`) stellt einen bewussten Kosten- und Laufzeit-Kompromiss dar: Sie gewährleistet schnelle Feedback-Zyklen und vertretbare Ressourcenanforderungen bei automatisierten Pull-Request-Gates, während die umfangreichere `_m`-Variante für optionale, manuelle Tiefenläufe reserviert bleibt.

Detaillierte Anleitungen zur lokalen Ausführung und methodische Details sind in [`benchmarks/README.md`](benchmarks/README.md) dokumentiert.

## Architektur-Entscheidungen (ADRs)

Die vollständige Begründung zur Fokussierung auf die Python/Rust-Library und der Deprecation der Desktop-Anwendung ist in [ADR-077 in DECISIONS.md](DECISIONS.md#adr-077-produktvision-pypi-library-fokus-und-tauri-deprecation) dokumentiert. Siehe [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) für die komplette System- und Crate-Architektur.

## Lizenz

MIT OR Apache-2.0
