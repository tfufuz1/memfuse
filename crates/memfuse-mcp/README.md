# MemFuse MCP Server (`memfuse-mcp`)

Model Context Protocol (MCP) Server für **MemFuse Brain** — Ermöglicht es KI-Agenten (wie Claude Desktop, Cursor oder custom MCP-Clients), lokal über das standardisierte Model Context Protocol auf die 4-Signal RAG-Engine von MemFuse zuzugreifen.

---

## 1. Installation & Build-Anleitung

### Voraussetzungen

- Rust Toolchain (`cargo`, 1.80+)
- Ein laufendes [Ollama](https://ollama.com) Instanz (Standard-URL: `http://localhost:11434`) mit installiertem Embedding-Modell (z. B. `nomic-embed-text`)

### Binary bauen

Bauen Sie den MCP-Server aus dem Root-Verzeichnis des Repositories:

```bash
cargo build -p memfuse-mcp --release --bin memfuse-mcp-server
```

Das kompilierte Executable befindet sich nach erfolgreichem Build unter:

```
target/release/memfuse-mcp-server
```

*(Auf Windows: `target\release\memfuse-mcp-server.exe`)*

---

## 2. CLI-Schnittstelle & Umgebungsvariablen

Der Server kommuniziert ausschließlich über Standard I/O (stdio) via JSON-RPC 2.0 (ADR-010).

### Kommandozeilenargumente (CLI-Flags)

| Flag | Standardwert | Beschreibung |
|---|---|---|
| `--db-path <PFAD>` | `./memfuse_data` | Pfad zum MemFuse-Datenbankverzeichnis |
| `--allow-write` | *deaktiviert* | Schreibende Operationen (`memfuse_insert`) erlauben |
| `--read-only` | *aktiviert* | Erzwingt Read-Only-Modus (Schreibzugriffe gesperrt) |
| `--provider <TYPE>` | `ollama` | Embedding-Provider (`ollama`, `onnx`, `mock`) |
| `--ollama-url <URL>` | `http://localhost:11434` | Ollama-Server URL |
| `--embed-model <NAME>` | `nomic-embed-text` | Name des Ollama Embedding-Modells |
| `--onnx-model-path <PFAD>` | *keiner* | Pfad zum ONNX-Modell (nur bei ONNX-Feature) |

### Umgebungsvariablen

| Variable | Werte | Beschreibung |
|---|---|---|
| `MEMFUSE_MCP_ALLOW_WRITE` | `1`, `true`, `yes` | Schreibende Operationen aktivieren (falls kein CLI-Flag angegeben) |
| `MEMFUSE_OLLAMA_URL` | z. B. `http://localhost:11434` | Alternativer Ollama Endpoint |
| `MEMFUSE_EMBED_MODEL` | z. B. `nomic-embed-text` | Alternativname für Embedding-Modell |
| `MEMFUSE_EMBEDDING_PROVIDER` | `ollama`, `onnx`, `mock` | Alternative Provider-Spezifikation |

---

## 3. Konfiguration für Claude Desktop

Fügen Sie den Server in Ihre Claude Desktop Konfigurationsdatei ein:

- **macOS**: `~/Library/Application Support/Claude/claude_desktop_config.json`
- **Windows**: `%APPDATA%\Claude\claude_desktop_config.json`
- **Linux**: `~/.config/Claude/claude_desktop_config.json`

### Beispiel: `claude_desktop_config.json`

```json
{
  "mcpServers": {
    "memfuse": {
      "command": "/ABSOLUTER/PFAD/ZU/target/release/memfuse-mcp-server",
      "args": [
        "--db-path",
        "/ABSOLUTER/PFAD/ZU/ihrem_datenbank_ordner",
        "--allow-write",
        "--ollama-url",
        "http://localhost:11434",
        "--embed-model",
        "nomic-embed-text"
      ],
      "env": {
        "MEMFUSE_MCP_ALLOW_WRITE": "1"
      }
    }
  }
}
```

> **Wichtig:** Ersetzen Sie `/ABSOLUTER/PFAD/ZU/...` durch die tatsächlichen absoluten Pfade auf Ihrem System!

---

## 4. Schritt-für-Schritt Demo

Hier ist eine minimale Demonstration des MCP-Protokolls über stdio.

### 1. Server im schreibfähigen Modus starten

```bash
target/release/memfuse-mcp-server --db-path ./demo_db --allow-write
```

*(Der Server wartet nun auf JSON-RPC 2.0 Anfragen über stdin.)*

### 2. Verfügbare Tools abfragen (`tools/list`)

Senden Sie folgende Zeile an `stdin`:

```json
{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}
```

**Erwartete Antwort (stdout):**

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "tools": [
      {
        "name": "memfuse_search",
        "description": "Hybrid semantic search (vector + BM25 + graph) über gespeicherte Dokumente...",
        "inputSchema": { ... }
      },
      {
        "name": "memfuse_insert",
        "description": "Dokument einspeichern (auto-embedding, auto-chunking mit MarkdownChunker, ~512 Tokens).",
        "inputSchema": { ... }
      },
      {
        "name": "memfuse_get",
        "description": "Dokument per ID abrufen...",
        "inputSchema": { ... }
      },
      {
        "name": "memfuse_collections",
        "description": "Alle Collections auflisten.",
        "inputSchema": { ... }
      }
    ]
  }
}
```

### 3. Dokument einspeichern (`memfuse_insert`)

Senden Sie eine Aufrufanfrage für `memfuse_insert`:

```json
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"memfuse_insert","arguments":{"id":"doc-firma-01","text":"Unsere Urlaubsregelung sieht 30 Tage Jahresurlaub vor. Urlaubsanträge müssen 2 Wochen im Voraus eingereicht werden.","collection":"hr_docs"}}}
```

**Erwartete Antwort (stdout):**

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "result": {
    "content": [
      {
        "type": "text",
        "text": "{\"chunk_ids\":[\"doc-firma-01\"],\"chunks_inserted\":1,\"collection\":\"hr_docs\",\"id\":\"doc-firma-01\",\"ok\":true}"
      }
    ]
  }
}
```

### 4. Hybridsuche ausführen (`memfuse_search`)

Senden Sie eine Suchanfrage an die Collection `hr_docs`:

```json
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"memfuse_search","arguments":{"query":"Wie viele Tage Urlaub stehen mir zu?","collection":"hr_docs","k":1}}}
```

**Erwartete Antwort (stdout):**

```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "result": {
    "content": [
      {
        "type": "text",
        "text": "[{\"content_provenance\":\"retrieved_untrusted_data\",\"id\":\"doc-firma-01\",\"metadata\":{\"chunk_index\":0,\"chunk_total\":1,\"source_id\":\"doc-firma-01\",\"text\":\"Unsere Urlaubsregelung sieht 30 Tage Jahresurlaub vor. Urlaubsanträge müssen 2 Wochen im Voraus eingereicht werden.\"},\"score\":0.95}]"
      }
    ]
  }
}
```

---

## 5. Exponierte MCP-Tools

Der Server exponiert 4 Kern-Tools:

1. **`memfuse_search`**
   - **Parameter:** `query` (string, required), `collection` (string, default: `"default"`), `k` (integer, default: 10)
   - **Funktion:** Sendet eine Hybridsuchanfrage (Vector + BM25 + Wissensgraph).

2. **`memfuse_insert`**
   - **Parameter:** `id` (string, required), `text` (string, required), `collection` (string, default: `"default"`), `metadata` (object, optional)
   - **Funktion:** Speichert ein Dokument ein. Teilt längere Texte automatisch mittels `MarkdownChunker` in Abschnitte auf und generiert Embeddings.

3. **`memfuse_get`**
   - **Parameter:** `id` (string, required), `collection` (string, default: `"default"`)
   - **Funktion:** Ruft ein spezifisches Dokument anhand seiner ID ab.

4. **`memfuse_collections`**
   - **Parameter:** keine
   - **Funktion:** Listet alle vorhandenen Collections in der Datenbank auf.

---

## 6. Troubleshooting & Häufige Fehler

### Fehler: `Sandbox: DB-Schreibzugriff gesperrt`
- **Ursache:** Schreibende Operationen (`memfuse_insert`) werden von der `McpSandbox` standardmäßig blockiert.
- **Lösung:** Starten Sie das Binary mit dem Flag `--allow-write` oder setzen Sie die Umgebungsvariable `MEMFUSE_MCP_ALLOW_WRITE=1`.

### Fehler: Ollama-Verbindung fehlgeschlagen (`connection refused`)
- **Ursache:** Ollama läuft nicht oder ist unter der angegebenen URL nicht erreichbar.
- **Lösung:**
  1. Stellen Sie sicher, dass Ollama läuft (`ollama serve` oder Ollama App gestartet).
  2. Überprüfen Sie, ob das Embedding-Modell vorhanden ist: `ollama pull nomic-embed-text`.
  3. Prüfen Sie die URL via `--ollama-url http://localhost:11434`.

### Fehler: Protokollstörung durch Log-Ausgaben (JSON Parse Error in Claude)
- **Ursache:** stdout ist ausschließlich für JSON-RPC-Protokollnachrichten reserviert.
- **Lösung:** Der Server leitet alle Tracing-Logs automatisch strikt nach `stderr`. Stellen Sie sicher, dass eigene Erweiterungen oder Wrapper nicht nach `stdout` drucken.

### Fehler: Database Lock / Permission Denied
- **Ursache:** Das Datenbankverzeichnis ist von einem anderen Prozess blockiert oder es fehlen Schreibrechte.
- **Lösung:** Stellen Sie sicher, dass keine zweite Instanz von MemFuse exklusiv auf `--db-path` zugreift und die Verzeichnisrechte stimmen.
