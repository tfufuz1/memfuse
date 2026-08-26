# MemFuse Brain

**Ihr lokaler, air-gapped Unternehmensassistent & 4-Signal Memory Engine.**

MemFuse Brain ist eine Desktop-Applikation und eingebettete Memory-Engine,
die Ihre Firmendokumente (PDF, Word, Markdown, E-Mails) durchsuchbar macht und
über ein lokal laufendes Sprachmodell (via Ollama) Fragen dazu beantwortet —
komplett offline, ohne dass ein einziges Byte Ihrer Daten das Gerät verlässt.

> ⚠️ **Status: Aktive Entwicklung.** Kern-Suchengine ist produktionsreif
> verifiziert (LSM-Tree, HNSW, BM25, CSR-Graph-Persistenz). Desktop-App (Tauri),
> MCP-Server und Ollama-Integration sind im Workspace vollständig integriert.

## Warum MemFuse Brain?

- **Air-Gapped by Design** — keine Cloud, keine Telemetrie, kein API-Key nötig
- **Zero-IT-Setup** — ein Installer, fertig. Kein Docker, kein Server, kein Admin
- **4-Signal-Hybridsuche** — Vektorsuche (HNSW) + Volltextsuche (BM25) +
  Wissensgraph (CSR) + Metadaten-Filter, fusioniert via Reciprocal Rank Fusion (RRF)
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

## Workspace Crates (12 Active Crates)

- **Layer 0**: `memfuse-core` (Typen, Traits, Error)
- **Layer 1**: `memfuse-store` (LSM-Tree), `memfuse-index` (HNSW), `memfuse-text` (BM25), `memfuse-crypto` (AES-GCM), `memfuse-graph` (CSR Graph), `memfuse-checkpoint` (Snapshotting)
- **Layer 2**: `memfuse-db` (Collections & 4-Signal Fusion)
- **Layer 3**: `memfuse-py` (Python PyO3 Bindings), `memfuse-ollama` (Ollama Client & Embeddings)
- **Layer 4**: `memfuse-mcp` (MCP Server), `memfuse-tauri` (Desktop App Shell)

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
cargo run -p memfuse-mcp --bin memfuse-mcp-server -- --db-path ./firma_daten
```

## Roadmap

- [x] LSM-Tree-Storage mit MVCC, WAL, Crash-Recovery
- [x] HNSW-Vektorindex mit SIMD-Beschleunigung
- [x] BM25-Volltextsuche mit deutscher Morphologie
- [x] CSR-Wissensgraph mit LSM-Persistenz (`__graph:`)
- [x] 4-Signal-Fusion (Vektor + BM25 + Wissensgraph + Metadaten) — persistiert & integriert
- [x] Dokumenten-Ingestion (PDF, DOCX, Markdown, E-Mail)
- [x] Tauri-Desktop-App (`memfuse-tauri`) & Ingestion-Pipeline
- [x] Ollama-Integration (`memfuse-ollama`) & Diagnostic Checks
- [x] Standalone MCP-Server (`memfuse-mcp`) mit stdio JSON-RPC 2.0 (ADR-010)

## Lizenz

MIT OR Apache-2.0
