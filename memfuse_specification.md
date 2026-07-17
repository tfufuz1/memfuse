# MemFuse — System Specification (Sovereign Core)

Dieses Dokument dient als zentrale, LLM-freundliche Projektspezifikation von **MemFuse** – einer air-gapped, zero-panic, Embedded Vector Engine, die 4-Signal Fusion (Vektor, Text, Graph, Metadaten) in einer ACID-konformen Architektur vereint.

---

## 1. System-Architektur (DAG-Schichtenmodell)

Das System basiert auf einer strikten, unidirektionalen DAG-Struktur (Directed Acyclic Graph), in der höhere Schichten Abhängigkeiten nach unten haben, niemals umgekehrt. 

### Layer 0: Foundation
*   **`memfuse-core`**: Definiert die Kern-Interfaces, Traits (z. B. `StorageEngine`, `VectorIndex`), Fehler (`MemFuseError`), `TxId`, `DocId` und Transaktionspuffer. Garantiert 100% Mock-Fähigkeit für Unit-Tests.

### Layer 1: Engines & Persistenz
*   **`memfuse-store`**: LSM-Tree-Persistenz. Implementiert das Write-Ahead-Log (WAL) mit HMAC-Integrität und SSTables mit intelligentem Compaction-Scheduling (Fair-Selection nach Tier-Füllgrad).
*   **`memfuse-index`**: HNSW-Vektorsuche. Nutzt hardware-beschleunigtes SIMD (AVX-512, NEON) und SQ8-Quantisierung zur Dimensionsreduktion.
*   **`memfuse-text`**: BM25 Inverted Index für die Volltextsuche. Nutzt Atomics für lock-freie Statistiken (`total_docs`, `total_tokens`) und einen dedizierten Forward-Index zur Tombstone-Resolution.
*   **`memfuse-crypto`**: Stellt AES-GCM Verschlüsselung und HMAC (SHA-256) Schutz für den WAL bereit.
*   **`memfuse-graph`**: CSR (Compressed Sparse Row) Architektur für Entity-Relation Traversals. (BFS Edge-Isolation).

### Layer 2: Orchestrierung
*   **`memfuse-db`**: Die Kern-Datenbank. Orchestriert Layer 1. Implementiert **Snapshot-Isolation** (MVCC), Reciprocal Rank Fusion (RRF) und 2-Phasen-Commits (2PC) über Collections. Bietet die `MemFuse` Haupt-API.

### Layer 3: Integration (Extern)
*   **`memfuse-py`**: Python Bindings via PyO3, GIL-Release-optimiert durch Flatbuffer-Interprozesskommunikation (IPC).
*   **`memfuse-embed`** *(Optional)*: ONNX-basierte automatische Embedding-Generierung (HuggingFace Hub Support).
*   **`memfuse-cluster`** *(Optional)*: Raft-Konsens für verteilte Replikation.

### Layer 4: Frozen Zone (Optional / Erfordert §27)
*   **`memfuse-checkpoint`**, **`memfuse-saos-agent`**, **`memfuse-sandbox`**: Experimentelle oder stark limitierte Air-Gap Features.

---

## 2. Kern-Schnittstellen (Traits)

Die zentralen Schnittstellen (aus `memfuse-core/src/traits.rs`) sind `#[async_trait]` abstrahiert:

### StorageEngine
```rust
async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;
async fn get_at_seq(&self, key: &[u8], seq: u64) -> Result<Option<Vec<u8>>>;
async fn put_batch(&self, tx_id: TxId, entries: &[(Vec<u8>, Vec<u8>)]) -> Result<()>;
async fn scan_prefix_at(&self, prefix: &[u8], seq_no: u64) -> Result<Vec<(Vec<u8>, Vec<u8>)>>;
async fn commit(&self, tx_id: TxId) -> Result<()>;
```

### VectorIndex (HNSW)
```rust
async fn search(&self, query: &[f32], k: usize) -> Result<Vec<ScoredDocument>>;
async fn insert_batch(&self, tx: TxId, vectors: &[(DocId, &[f32])]) -> Result<()>;
```

### TextIndex (BM25)
```rust
async fn search(&self, query: &str, k: usize) -> Result<Vec<ScoredDocument>>;
async fn insert(&self, tx: TxId, id: DocId, text: &str) -> Result<()>;
```

### GraphIndex (CSR)
```rust
async fn traverse(&self, start_node: EntityId, max_hops: usize) -> Result<Vec<(EntityId, f32)>>;
async fn add_edge(&self, tx: TxId, edge: Edge) -> Result<()>;
```

---

## 3. Funktionierende Features (Production Ready)

Folgende Invarianten und Features sind im aktuellen Build erfolgreich verifiziert:
1. **Zero-Panic Doctrine**: Kompletter Verzicht auf `.unwrap()` und `.expect()` in Produktions-Schichten (Layer 0-2). Sämtliche Pfade münden in `MemFuseError` (bewiesen durch Audit).
2. **Snapshot-Isolation (MVCC)**: Suchanfragen (`scan_prefix_at`, `search_bm25`) lesen konsistent nur Datenpakete, die zum Zeitpunkt des Reads committet wurden (inklusive Snapshot-Guards in der Collection).
3. **Sovereign Code**: Reine Rust-Implementierung ohne verdeckte C-Abhängigkeiten im Hauptausführungspfad (Feature-Gated).
4. **4-Signal Fusion**: Erfolgreiche Verschmelzung von Vektor-, BM25- und Graph-Ergebnissen über RRF im `memfuse-db` Modul.
5. **Numerical Determinism**: SIMD-Instruktionen produzieren identische Werte zum Skalar-Fallback (Abweichung < 1e-4).
6. **Flatbuffer IPC**: Python-to-Rust Bridge serialisiert Ergebnisse asynchron ohne das Python-GIL zu blockieren (ausgelagert nach `memfuse-core/ipc`).

---

## 4. Audit Findings & Schwachstellen-Analyse

Das Projekt hat drei intensive Security- und Architektur-Audits durchlaufen (Sprint 1 bis 3). Hier ist der detaillierte Status aller entdeckten Schwachstellen:

### ✅ Bereits gelöste Schwachstellen (Geheilt)
Diese kritischen Lücken wurden erfolgreich behoben und verifiziert:
*   **`memfuse-core`**: 
    *   Zero-Division Panics im `TxBuffer` wurden durch Absicherungen eliminiert.
    *   Cosine-Distanz und Negative-Weights Berechnungen wurden für `u8` und `f32` mathematisch sicher gemacht.
*   **`memfuse-store`**:
    *   **Phantom-Daten (FIND-STO-001):** Aggressive Tombstone-Garbage-Collection wurde gefixt. Tombstones werden nun sicher zurückgehalten (`retain_tombstone`), wenn in tieferen Tiers noch alte Daten existieren.
    *   Tier-Backlog-Selection wurde auf `fill_ratio` umgestellt, was Compaction-Staus verhindert.
    *   SSTable-Formate besitzen nun eine Versions-Signatur zur robusten CRC-Validierung.
*   **`memfuse-text`**:
    *   **Dirty Reads (FIND-TXT-001):** Der Suchpfad ist nun über `scan_prefix_at` an Snapshot-Guards gebunden, was "Phantom Reads" ausschließt.
    *   **Lock Contention (FIND-TXT-004):** Metadaten (Token/Doc-Count) blockieren keine Reads mehr, da sie über `AtomicU64` in Form eines `StagedStatsChange` abgehandelt werden.
    *   Gefährliche `unwrap_or_default()` Aufrufe beim Parsen des Forward-Indexes wurden durch sicheres `Result`-Routing (`map_err`) ersetzt.
*   **`memfuse-index`**: 
    *   Endians-Safe Persistenz (Little-Endian) beim HNSW-Speichern eingebaut (FIND-IND-003).
    *   SIMD-Determinismus-Abweichungen wurden quantifiziert und durch Toleranz-Verträge (< 1e-6) abgesichert.
*   **`memfuse-crypto`**: 
    *   Falsche Offsets beim HMAC-Verifizierungsfehler behoben.
*   **`memfuse-db`**: 
    *   Storage Leaks bei `drop_collection` wurden durch `Collection::cleanup()` korrigiert.
    *   Sandbox-Brücken von verbliebenen `unwrap()`-Aufrufen bereinigt (Zero-Panic).

### ❌ Offene Schwachstellen (To-Do)
Diese Findings aus Sprint 2 & 3 sind noch architektonische Lücken und müssen vom nächsten Agenten geschlossen werden:

#### Sprint 2 (Data-Integrity Restarbeiten)
*   **FIND-STO-004 (FSync Lücke):** In `memfuse-store/src/wal.rs` fehlt der `fsync` auf das WAL-Parent-Verzeichnis nach dem Erstellen der `.uuid` Datei.
*   **FIND-DB-005 (Split-Brain in 2PC):** Die 2-Phasen-Commit Logik besitzt kein Recovery-Log. Fällt die DB während des Commits aus, entsteht Inkonsistenz. Es muss ein Commit-Intent Namespace (`__tx_intent:{tx_id}`) in den LSM-Tree geschrieben werden.
*   **FIND-IND-002 (Quantisierungs-Präzisionsverlust):** `SQ8` nutzt globale Min/Max Werte. Dies muss auf Per-Dimension `per_dim_min`/`max` umgeschrieben werden, um massiven Recall-Verlust bei asymmetrischen Vektoren zu stoppen.
*   **FIND-DB-004 (Repair Bottleneck):** Der HNSW-Repair Mechanismus in der Collection ist zu langsam (iteriert per Suche statt O(1) über `doc_to_node`).

#### Sprint 3 (Cluster & Graph Architektur-Remediation)
*   **FIND-CLU-001 (Index Blindheit):** Follower-Knoten im Raft-Cluster schreiben Daten am `memfuse-db` vorbei direkt in den `memfuse-store`. Dadurch werden die HNSW- und BM25-Indizes auf Replikas *nicht* aktualisiert.
*   **FIND-CLU-002 & 003 (Raft Ephemeral Log):** Der Raft-Log persistiert aktuell nicht (nur In-Memory BTreeMap). Ein Neustart führt zum Cluster-Verlust.
*   **FIND-GRA-001 & 002 (Graph Volatility & Compaction):** Der Graph-Index (`CSR`) hat keine Speicherlogik (`save`/`load`). Bei Neustart ist der Graph weg. Bei Writes (Edges) entsteht aktuell eine $O(N+E)$ Full-Rebuild Belastung.
*   **FIND-PY-001 & 002 (Fassaden-Gesetz-Bruch):** Die Python-Fassade serialisiert FlatBuffer-Objekte selbst (Bruch von DAG-Regel §20) und blockiert das GIL. Die Serialisierung muss nach `memfuse-db/src/ipc.rs` verlagert werden.
*   **FIND-EMB-001 (Souveränitätsrisiko):** Der automatische Download-Pfad (`from_hub()`) bricht das Air-Gap Paradigma. Er muss strikt hinter einem `feature = "fetch"` Flag in der `Cargo.toml` isoliert werden.
*   **FIND-FRZ-001 (Sandbox Isolation):** Es fehlt ein sicherer Shared-Memory Rückkanal (Host-Function) für die Wasm-Sandbox zur Übermittlung von Suchergebnissen an den Host.

---

## 5. Benchmarks (Criterion Suite)

Das Projekt nutzt `criterion` für Micro- und Macro-Benchmarks in `benches/migration_benchmarks.rs`. 
Das Ziel dieser Benchmark-Suite ist der Beweis der niedrigen Latenz (im Vergleich zu Redis/Chroma) in Agenten-Migrationen (z. B. LangGraph).

Folgende Benchmarks existieren und laufen erfolgreich unter Last:

*   **`hybrid_search_latency`**: 
    *   *Operation:* `db.hybrid_search(keyword, vector, k)`
    *   *Ziel:* Messung der Verschmelzungsgeschwindigkeit der HNSW- und BM25-Engines inkl. RRF-Scoring bei 1536-dimensionalen Vektoren.
*   **`checkpoint_latency`**: 
    *   *Operation:* `manager.create_checkpoint(...)` 
    *   *Ziel:* Persistieren von komplexen Agenten-Zuständen (JSON-Payloads) als Snapshots über den LSM-Store.
*   **`rerun_cost_get_latency`**: 
    *   *Operation:* `db.get(doc_id)`
    *   *Ziel:* O(1) Key-Value Lookup-Zeiten für direkte Document-Fetches (Latenzkosten für "Agent-Reruns").
