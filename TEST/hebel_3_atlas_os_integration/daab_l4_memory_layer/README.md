# Atlas OS L4 Memory Schicht (DaaB — Database-as-a-Brain)

## 1. Ausgangslage in Atlas OS
In Atlas OS (`apps/kernel/src_agents/daab/`) ist die L4-Persistenz zweigeteilt:
1. **SQLite (via `aiosqlite`):** Speichert relationale Daten, Audit-Logs, User-Settings und LangGraph Checkpoints (`~/.atlas/data/atlas.db`).
2. **LanceDB:** Speichert Vektoren und semantische Embeddings (`~/.atlas/data/vectors`).
3. **FTS5 & Hybrid Search:** Python-Code in `hybrid_search.py` versucht, SQLite-Keyword-Treffer und LanceDB-Vektortreffer ad-hoc zusammenzuführen.

### Schwachstellen der aktuellen Atlas-Lösung:
- **Fragmentierte Transaktionen:** Keine gemeinsame Transaktionssicherheit (ACID) zwischen SQLite und LanceDB. Bei Absturz droht Desynchronisation.
- **Python-Overhead:** Hybrid-Search-Fusion, Reranking und Filterung laufen in Python und belasten den Event-Loop.
- **Speicher-Overhead:** Zwei getrennte Caching- und Speicher-Engines im RAM.

## 2. Die MemFuse-Synergie
MemFuse vereint Vektorsuche (HNSW), Volltext (BM25), Wissensgraph (CSR) und Metadaten in einer einzigen, extrem schnellen Rust-Engine mit MVCC und WAL.

Durch die Ablösung der fragmentierten LanceDB+SQLite-Schicht durch **MemFuse (via PyO3)** erhält Atlas:
- **Einheitliche Transaktionen:** Atomare Writes für Text, Metadaten und Vektoren.
- **Microsekunden-Latenz:** Native Rust-Pipeline statt Python-Glue-Code.
- **4-Signal-Fusion:** Hochwertigere Antworten für LangGraph-Agenten.

## 3. Extrahierte Original-Komponenten

| Datei | Beschreibung |
|:---|:---|
| [`core.py`](./core.py) | Original DaaB-Klasse mit LanceDB- und SQLite-Verbindung |
| [`interface.py`](./interface.py) | Abstraktes Memory-Provider-Interface von Atlas |
| [`models.py`](./models.py) | Datenmodelle für Memory-Einträge und Abfragen |
| [`hybrid_search.py`](./hybrid_search.py) | Atlas-eigene Python-Implementierung der Hybrid-Suche |
| [`memory_manager.py`](./memory_manager.py) | Kurzzeit- und Langzeit-Memory-Manager |
| [`sqlite_manager.py`](./sqlite_manager.py) | SQLite Pool- und Schema-Manager |
| [`snapshots.py`](./snapshots.py) | Checkpoint- und Rollback-Verwaltung |
| [`isolation.py`](./isolation.py) | Multi-Agent Namespace-Isolation |
| [`init_daab.sql`](./init_daab.sql) | DDL-Schema für Tabellen |
| [`AGENTS.md`](./AGENTS.md) | Original-Architektur-Dokumentation für Agenten |
