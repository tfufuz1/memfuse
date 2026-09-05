# Hebel 7: Live Agent-to-UI (A2UI) Streaming (aus Atlas OS)

## 1. Ausgangslage & Optimierungspotenzial für MemFuse
In vielen RAG- und Desktop-Systemen (einschließlich der bisherigen MemFuse Tauri UI) werden Abfrageergebnisse als einfache Textblöcke oder unformatierte Markdown-Antworten zurückgegeben.

**Atlas OS** hat dafür das **A2UI (Agent-to-User-Interface)** Protokoll entwickelt:
- **Type-Safe Dynamic UI Components:** Agenten und Retrieval-Engines geben strukturierte UI-Bäume (Cards, Badges, Data Tables, Provenance-Citations, Relevance Gauges) als JSON aus.
- **Progressives Event-Streaming:** Über Server-Sent Events (SSE) oder Tauri-Events (`window.emit`) wird das UI schrittweise aufgebaut, während der Reranker noch rechnet.
- **Kein hartcodiertes HTML:** Das Frontend rendert native Web Components (Lit / Svelte / Angular) dynamisch basierend auf dem A2UI JSON-Schema.

## 2. Extrahierte Komponenten

| Datei | Quelle | Beschreibung |
|:---|:---|:---|
| [`builder.py`](./builder.py) | `atlas/apps/kernel/src_agents/a2ui/builder.py` | Type-Safe Builder API zur Erstellung von UI-Bäumen (Cards, Badges, Rows, Grids) |
| [`models.py`](./models.py) | `atlas/apps/kernel/src_agents/a2ui/models.py` | Pydantic Datenmodelle aller A2UI-Komponenten |
| [`emitter.py`](./emitter.py) | `atlas/apps/kernel/src_agents/a2ui/emitter.py` | Asynchroner Streaming Event Emitter |
| [`stream_manager.py`](./stream_manager.py) | `atlas/apps/kernel/src_agents/a2ui/stream_manager.py` | Sitzungsbasierte Verwaltung aktiver UI-Streams |
| [`QUICK_REFERENCE.md`](./QUICK_REFERENCE.md) | `atlas/apps/kernel/src_agents/a2ui/QUICK_REFERENCE.md` | Schnellreferenz für alle A2UI-Komponenten |
| [`a2ui_memfuse_card.py`](./a2ui_memfuse_card.py) | Neu erstellt | Wandelt MemFuse 4-Signal Suchergebnisse in interaktive A2UI-Karten um |

## 3. Nutzen für MemFuse
In `memfuse-tauri` und bei MCP-Aufrufen können Treffer aus der 4-Signal-Fusion (Vektor-Score, BM25-Terms, Graph-Beziehungen, Metadaten) mit einem Klick in interaktive A2UI-Karten umgewandelt werden, die in der Desktop-App visuell beeindruckend dargestellt werden.
