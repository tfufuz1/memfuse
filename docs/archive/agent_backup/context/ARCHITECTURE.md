# MemFuse Architecture Context (The Sovereign Core)

## Das Ziel (The "Why")
MemFuse ist eine in-process, einbettbare und extrem performante Vektor/Hybrid-Suchdatenbank für lokale LLM-RAG-Systeme ("SQLite for AI Agents").

## Kern-Philosophie: Sovereign Core Doctrine
1. **Zero-Panic Policy:** Absolut kein `.unwrap()`, `.expect()` oder `panic!()` in Hot-Paths inkl. Type-Casts (`try_into()`). Fehlerfortpflanzung erfolgt strikt über `memfuse_core::MemFuseError`.
2. **Stable Rust Doctrine:** Das Projekt muss zwingend auf dem `stable` Toolchain kompilieren. `#![feature(...)]` Flags für Nightly-Features sind systemweit verboten.
3. **Keine blockierende I/O:** Es wird ausschließlich `tokio::fs` und `tokio::io` verwendet. Standard `std::fs` ist in asynchronen Kontexten absolut verboten, es sei denn, es wird per `tokio::task::spawn_blocking` isoliert.
4. **Isolierte Unsicherheit:** `unsafe` ist nur für Leistungsoptimierungen gestattet und muss zwingend mit `// SAFETY: [Beweis]` dokumentiert werden.
5. **Triple-Test-Gate:** Qualität wird durch 3x aufeinanderfolgende erfolgreiche Testläufe und Zero-Clippy-Warnings sichergestellt.

## Crate-Hierarchie (4-Layer DAG)

MemFuse folgt einer strikten Schichtenarchitektur. Abhängigkeiten dürfen nur nach unten zeigen.

### Schicht 3: Interface (User-Facing)
- **`memfuse-py`**: Python-Bindings (PyO3). Das primäre Tor für Anwendungsentwickler.

### Schicht 2: Orchestration & SAOS
- **`memfuse-db`**: Die zentrale Facade. Orchestriert Storage, Index und Text-Suche. Handhabt Collections und Fusion (RRF).
- **`memfuse-saos-agent`**: Agent-Workflow-Engine (StateGraph). Deterministische Schrittfolge für autonome Agenten.
- **`memfuse-checkpoint`**: Snapshot-Registry für Time-Travel und MVCC-basiertes Workflow-Freezing.
- **`memfuse-sandbox`**: WASM-Sandbox zur sicheren Ausführung von Agent-Tools ohne Host-Zugriff.

### Schicht 1: Sub-Engines (Engine Room)
- **`memfuse-store`**: LSM-Storage (MemTables, SSTables, WAL). Zuständig für Persistenz und atomare Schreibvorgänge.
- **`memfuse-index`**: Semantische Suche (HNSW-Graph, SQ8 Quantization).
- **`memfuse-text`**: Lexikalische Suche (BM25 Inverted Index, Tokenizer).
- **`memfuse-graph`**: CSR-Graph für Entity-Relation Traversal (Signal 3 der Fusion).
- **`memfuse-crypto`**: Ver- und Entschlüsselung (AES-GCM) für Encryption-at-Rest.

### Schicht 0: Kernel
- **`memfuse-core`**: Das schlagende Herz. Definiert `MemFuseError`, fundamentale Traits (`StorageEngine`, `VectorIndex`), `TxBuffer` und `DocId`. Importiert keine anderen Crates aus dem Projekt.

## Concurrency & State Management
- **Async First:** Alles ist auf `tokio` optimiert.
- **Thread-Safety:** Einsatz von `parking_lot::RwLock` für effiziente Lesezugriffe.
- **Atomarität:** Commits erfolgen transaktional über den `TxBuffer` in `memfuse-core`, synchronisiert durch `memfuse-db`.

## Data Flow
1. **Write:** `Collection::insert` → `TxBuffer` → `WAL` (store) → `MemTable` (store) + `Index/Text Update`.
2. **Search:** `Collection::search` → `HybridSearch` (db) → Parallel: `HnswSearch` (index) + `BM25Search` (text) → `RRF Fusion` (db) → Ergebnisliste.
