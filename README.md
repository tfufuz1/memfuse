# MemFuse Brain

**Ihr lokaler, air-gapped Unternehmensassistent — mit professionellem Gedächtnis.**

MemFuse Brain ist eine Desktop-Applikation, die Ihre Firmendokumente
(PDF, Word, Markdown, E-Mails) durchsuchbar macht und über ein lokal
laufendes Sprachmodell (via Ollama) Fragen dazu beantwortet — komplett
offline, ohne dass ein einziges Byte Ihrer Daten das Gerät verlässt.

> ⚠️ **Status: Aktive Entwicklung.** Kern-Suchengine ist produktionsreif
> getestet (LSM-Tree, HNSW, BM25). Desktop-App und Ollama-Integration
> befinden sich im Aufbau.

## Warum MemFuse Brain?

- **Air-Gapped by Design** — keine Cloud, keine Telemetrie, kein API-Key nötig
- **Zero-IT-Setup** — ein Installer, fertig. Kein Docker, kein Server, kein Admin
- **3-Signal-Hybridsuche** — Vektorsuche (HNSW) + Volltextsuche (BM25) +
  Wissensgraph, fusioniert via Reciprocal Rank Fusion
- **Deutsche Morphologie** — versteht "Urlaubsantragsprozess" auch als
  "Urlaub", "Antrag", "Prozess" für bessere Trefferqualität
- **Verschlüsselt** — AES-256-GCM auf Disk, HMAC-Anti-Tamper im WAL

## Installation

### Systemanforderungen

- Windows 10/11, macOS 11+, oder eine gängige Linux-Distribution
- [Ollama](https://ollama.com) muss separat installiert und gestartet sein
  (MemFuse Brain nutzt Ollama als lokales LLM-Backend)
- Mindestens ein Ollama-Modell heruntergeladen, z.B.:
```bash
  ollama pull llama3.2
  ollama pull nomic-embed-text
```

### Installer herunterladen

Native Installer für Windows (.msi/.exe), macOS (.dmg) und Linux
(.AppImage/.deb) werden bei jedem Release unter GitHub Releases
bereitgestellt.

### Aus dem Quellcode bauen

```bash
cd crates/memfuse-tauri
cargo tauri build
```

## Architektur

```
┌─────────────────────────────────────────┐
│  MemFuse Brain (Tauri Desktop-App)       │
│  ┌─────────────┐  ┌────────────────────┐│
│  │ Chat-UI      │  │ Dokumenten-Import  ││
│  └──────┬───────┘  └─────────┬──────────┘│
│         │                     │            │
│  ┌──────▼─────────────────────▼─────────┐ │
│  │  Ollama-Bridge (lokales LLM)         │ │
│  └──────┬─────────────────────────────  ┘ │
│         │                                  │
│  ┌──────▼─────────────────────────────┐   │
│  │  MemFuse Core (3-Signal RAG-Engine) │   │
│  │  Vektor + BM25 + Wissensgraph        │   │
│  └───────────────────────────────────┘    │
└─────────────────────────────────────────┘
         Alles lokal. Nichts verlässt den Rechner.
```

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

```bash
cargo run --bin memfuse-mcp-server -- --db-path ./firma_daten
```

## Roadmap

- [x] LSM-Tree-Storage mit MVCC, WAL, Crash-Recovery
- [x] HNSW-Vektorindex mit SIMD-Beschleunigung
- [x] BM25-Volltextsuche mit deutscher Morphologie
- [x] 3-Signal-Fusion (Vektor + BM25 + Wissensgraph) — persistiert & integriert
- [x] Dokumenten-Ingestion (PDF, DOCX, Markdown, E-Mail)
- [ ] Tauri-Desktop-App (UI in aktivem Aufbau)
- [ ] Ollama-Chat-Integration mit Streaming
- [ ] MCP-Server (axum/SSE)
- [ ] Native Installer (Windows/macOS/Linux)

## Lizenz

MIT OR Apache-2.0
