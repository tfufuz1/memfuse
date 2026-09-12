# MemFuse — Gesamtspezifikation des Endprodukts für Large Language Models

> **Status:** Verbindlich · Normative Architektur- & Schnittstellenspezifikation für LLM-Agenten
> **Stand:** 2026-09-12 · Vollständige Quellcode-Abbildung aller 18 Workspace-Crates, des `memfuse-py` FFI-Workspaces sowie der `xtask`- & `memfuse-bench`-Werkzeug-Crates
> **Geltungsbereich:** Dieses Dokument ist die einzige normative Referenz für die Architektur, Datenstrukturen, Schnittstellen, Invarianten, Concurrency-Modelle, Sicherheitsinfrastrukturen und Datenflüsse von MemFuse. Es ersetzt jegliche manuelle Codebase-Analyse für LLMs und bietet eine exakte, vollständige Abbildung aller Komponenten.

---

## §1 — Kernthese & Produktvision

### §1.1 Was MemFuse ist
MemFuse ist die technisch fortschrittlichste, vollständig lokal betriebene Gedächtnisschicht für KI-Agenten. Sie fungiert als eingebettete, hochperformante Cognitive OS Infrastruktur, die Multi-Signal-Retrieval, unüberwachte Konsolidierung ("Sleep Cycles"), proaktive Kalibrierung und kryptographische Integrität vereint.

MemFuse ist reine Infrastruktur. Es wird über drei Schnittstellen verteilt:
1. **`memfuse-mcp`** (Primärer Eingang): MCP-Server für Agenten-Umgebungen (Claude Desktop, Cursor, Windsurf, Cline), paketierbar via `uvx`.
2. **`memfuse` (Python-Library)**: Python-FFI-Bindings via PyO3 (`pip install memfuse`) mit GIL-Freigabe und Sub-Interpreter-Isolierung nach PEP 684.
3. **`memfuse` (Rust-Crate)**: High-Level-Rust-API (`crates.io`) für native Rust-Agentensysteme.

### §1.2 Primäre Distributionswege & Zielarchitektur
- `memfuse-mcp`: Aufrufbar über `uvx memfuse-mcp --db-path ~/.memfuse`.
- `memfuse-py`: FFI-Grenzschicht in `crates/memfuse-py`, kompilierbar als CPython-Erweiterungsmodul `_memfuse`.
- `memfuse` (Rust Crate): Direkte Einbindung der Workspace-Crates (allen voran `memfuse-db`).

### §1.3 Alleinstellungsmerkmale
1. **5-Signal Retrieval Fusion inkl. PathRAG & Synaptischer Co-Aktivierung:** HNSW (Vektor) + BM25 (Volltext mit deutscher Komposita-Dekomposition) + CSR-Graph (Personalized PageRank) + Metadaten-Filter + Synaptische Edge-Co-Aktivierung, fusioniert via Reciprocal Rank Fusion (RRF) mit Resonanz-Kohärenz-Bonus ($\gamma \cdot (S/T)^\beta$). PathRAG (bidirektionaler Dijkstra) ermöglicht Multi-Hop-Traversierung.
2. **Kalibriertes Retrieval mit proaktiver Drift-Erkennung:** Isotonic-Kalibrierung (PAVA) und Lyapunov-Drift-Watcher erkennen Trajektorien-Qualitätsverschlechterungen proaktiv.
3. **Kryptographische Integrität & DSGVO Art. 17 Löschung:** WAL-HMAC-Kette mit Snapshots (`restore_last_hmac`) sowie fälschungssicherer `DeletionProof` für kryptographische Löschnachweise.
4. **Pure Rust Air-Gap Inferenz & Encrypted KV-Cache-Bridge:** Native Candle GGUF-Inferenz ohne externe Abhängigkeiten, gepaart mit mandantenisolierter LRU-verschlüsselter `KvSegmentStore` Bridge für LLM-Prefill-Bypass.
5. **Typsichere Lock-Hierarchie:** `SessionDag` mit `NodesGuard` erzwingt die Vermeidung von Deadlocks zur Compile-Zeit.
6. **Portables Memory-Export-/Import-Format:** Idempotenter JSON v1 Export und Import des gesamten Wissensgraphen und Vektorbestands.

### §1.4 Verbindliche Nicht-Ziele
- Kein Cloud-SaaS und kein zentral gehosteter Dienst.
- Kein Multi-Tenant Enterprise Multi-Client-Betrieb (`TenantId` dient ausschließlich Prozess- und Test-Isolierung sowie kryptographischer Segmentierung).
- Kein eigenes LLM-Training oder Fine-Tuning.
- Keine grafische Desktop-Oberfläche (`memfuse-tauri` wurde physisch aus dem Workspace entfernt).

---

## §2 — Fundamentale Architektur- und Designprinzipien (P1–P20)

- **P1 (Memory Safety & Zero-Unsafe):** `#![forbid(unsafe_code)]` gilt strikt im gesamten Workspace.
- **P2 (Single-Writer / Multi-Reader MVCC):** Schreiboperationen werden über MemTable/WAL serialisiert; Leser arbeiten konfliktfrei auf unveränderlichen Snapshots (`SnapshotRegistry`).
- **P3 (Deterministic Storage & WAL Integrity):** Die Storage-Engine nutzt Append-Only WALs mit HMAC-SHA256 Bindung. Legacy V1/V2 WALs werden beim Öffnen automatisch auf V3 promoted.
- **P4 (Cryptographic Erasure):** DSGVO Art. 17 Löschungen generieren unveränderliche `DeletionProof` Strukturen mit BLAKE3- und HMAC-Signaturen.
- **P5 (Multi-Signal Hybrid Fusion):** Das Retrieval kombiniert 5 Signale über Reciprocal Rank Fusion (RRF) mit konfigurierbarem $k \ge 0.0$.
- **P6 (PathRAG Multi-Hop Navigation):** Bidirektionaler Dijkstra-Algorithmus ermittelt relationale Pfade zwischen Entitäten über CSR-Graphstrukturen.
- **P7 (Conformal Prediction Routing):** `RouterEngine` kalibriert Vertrauensintervalle über ein Beobachtungsfenster von mindestens 50 Abfragen (`window_total >= 50`).
- **P8 (Proactive Drift Surveillance):** `LyapunovDriftWatcher` berechnet kontinuierlich Lyapunov-Exponenten über Routing-Trajektorien zur Abweichungserkennung.
- **P9 (Air-Gap Inferenz & KV-Cache-Bridge):** Inferenz und Embeddings laufen lokal über Candle (`CandleInferenceEngine`). Berechnete KV-Cache-Tensoren werden im `KvSegmentStore` verschlüsselt hinterlegt.
- **P10 (Type-Safe Deadlock Prevention):** Der `SessionDag` erzwingt Locking-Reihenfolgen über den Typen-Wrapper `NodesGuard`.
- **P11 (Grounding Defense):** Die Halluzinationsprüfung erfolgt über den `ResponseGroundingValidator` (implementiert via `GaspValidator`).
- **P12 (Consolidated Background Maintenance):** Ein einziger periodischer Scheduler (`PhysioScheduler`) steuert Thermostat-Eviction, Percolation-Checks, MWUM-Replicator-Dynamics und NREM-Sleep-Cycles mit WAL-Intent-Tracking (`__physio_intent:tick`).
- **P13 (Generative Community Consolidation):** In der REM-Phase fasst der `run_structural_synthesis_pass` stabile Graph-Communities (`CommunityStabilityTracker`) zu synthetischen `MetaChunk`s zusammen.
- **P14 (Dynamic Weight Allocation):** `ReplicatorState` (MWUM) passt Signal-Gewichte dynamisch an.
- **P15 (Resonanz-Fusion & Coherence Boosting):** Feature `physio-resonance-fusion` vergibt einen multiplikativen Kohärenz-Bonus $\gamma \cdot (S/T)^\beta$ bei Multi-Signal-Treffern.
- **P16 (Safe FFI & PEP 684 Isolation):** `memfuse-py` gibt den Python GIL frei (`py.allow_threads()`), fängt Rust-Panics ab (`catch_unwind`) und isoliert Tokio-Runtimes pro Sub-Interpreter.
- **P17 (Zero-Trust MCP Sandbox):** `memfuse-mcp` filtert Eingaben über den `PromptInjectionGuard` und isoliert flüchtige Tool-Ausgaben im `VolatileSandbox`.
- **P18 (Atomic Pre-Execution Budget Reservation):** `OrchestratorEngine` reserviert Budgets vor der Ausführung via `AgentTool::estimated_cost`.
- **P19 (Portable Interoperability):** Vollständiger JSON v1 Export und Import für Wissensdatenbanken.
- **P20 (Automated Governance):** Integrierte Gates für Claim-Verwaltung (`cargo xtask claim`), Recall-Stabilität und Mutation-Scores.

---

## §3 — Mikrofeingranulare Crate-Spezifikationen

### §3.1 `memfuse-core-ipc-gen` — Layer 0 (Zero-Copy Serialization)
- **Rolle:** Generierte FlatBuffers-Serialisierungsschnittstellen für Inter-Prozess-Kommunikation und High-Speed-IPC.
- **Module:** `build.rs`, `src/lib.rs`, `src/memfuse_generated.rs`.
- **Hauptkomponenten & Types:**
  - FlatBuffer-Schema-Definitionen für Vektoren, Dokument-Payloads und IPC-Messages.
- **Invarianten & Performance:** Null-Kopie-Deserialisierung, strict binary alignment.

---

### §3.2 `memfuse-core` — Layer 1 (Fundament, Dependency-Root)
- **Rolle:** Unterstes Fundament des gesamten Systems. Enthalten sind Core-Domain-Typen, Common-Traits, Unified Errors und In-Memory Transaction Buffers.
- **Module:**
  - `error.rs`: `MemFuseError`, `Result<T>`
  - `error_dto.rs`: `MemFuseErrorDto`
  - `types/domain.rs`: `DocId`, `EntityId`, `TxId`, `TenantId`, `CollectionId`, `DecisionId`, `Vector`, `Metadata`, `Entity`, `Edge`, `DistanceMetric`. All Core-IDs sind `#[repr(transparent)] u64` Newtypes.
  - `types/budget.rs`: `Budget`, `TokenBudget`.
  - `types/filter.rs`: `Filter`, `FilterExpr`.
  - `types/importance.rs`: `ImportanceScore`.
  - `types/saos.rs`: `SearchQuery`, `ScoredDocument`, `ProvenanceRecord`.
  - `traits/mod.rs` & `traits/embedding.rs`: `StorageEngine`, `VectorIndex`, `TextIndex`, `GraphIndex`, `ResponseGroundingValidator`, `EmbeddingProvider`, `LlmTextGenerator`, `LlmTextGeneratorStreaming`.
  - `snapshot.rs`: `SnapshotGuard`, `SnapshotRegistry` (MVCC Read Isolation).
  - `seq_log.rs`: `SequenceLog`, `SeqLogEntry`, `SeqLogChange`.
  - `tx_buffer.rs`: `TxBuffer`, `IndexOp` (Sharded Staging Buffer mit Orphan Reaper).
  - `model_fingerprint.rs`: `ModelFingerprint`, `ConfigFingerprint`.
- **Kern-Traits Signatures:**
  - `ResponseGroundingValidator`: `fn score_grounding(&self, response: &str, sources: &[&str]) -> Result<f32>`
  - `LlmTextGenerator`: `fn generate_text(&self, prompt: &str) -> BoxFuture<'_, Result<String>>`
- **Invarianten:** Absolut zero `unsafe`. Kein I/O, async oder Netz-Code in `types`.

---

### §3.3 `memfuse-checkpoint` — Layer 2 (Snapshot Pinning & Isolation)
- **Rolle:** Verwaltet konsistente Read-Snapshots und unterdrückt verfrühtes Compaction-Trimming während aktiver Leser-Transaktionen.
- **Module:** `src/lib.rs`.
- **Hauptkomponenten & Types:**
  - `CheckpointManager`: Erstellt und verwaltet Transaktions-Pins.
  - `InstanceOrphanRegistry`: Instanz-spezifische Registratur verwaister Checkpoints und Pins (gemäß ADR-053/ADR-058 ohne globale Statics).
  - `PinGuard`, `CheckpointGuard`: RAII-Guards. Verfassen Mutation-Dirty-Flags (`is_dirty`) synchron im Speicher und führen I/O-Persistence verzögert via `flush_orphan_registry()` durch.
- **Invarianten:** Droppen eines Guards ist lock-frei und blockiert den Ausführungsthread nicht mit Festplatten-I/O. Fehler werden via `tracing::error!` protokolliert.

---

### §3.4 `memfuse-calibration` — Layer 2 (Statistische Kalibrierung & PID-Steuerung)
- **Rolle:** Algorithmen zur Kalibrierung von Konfidenz-Scores, PID-basierter Suchpool-Steuerung und Multi-Signal-Gewichtung.
- **Module:** `isotonic.rs`, `platt.rs`, `pid.rs`, `lib.rs`.
- **Hauptkomponenten & Types:**
  - `IsotonicCalibrator`: Non-parametrische Isotonische Regression über Pool-Adjacent-Violators Algorithm (PAVA) in amortisiert $O(n)$.
    - *Invariant INV-CAL-1:* Liefert `None` für Vorhersagen, solange `observations.len() < warmup_required`.
    - *Invariant INV-CAL-2:* Setzt Beobachtungen zurück, wenn sich der `ConfigFingerprint` ändert.
  - `PlattScaler`: Parametrische logistische Kalibrierung mit Platt-Smoothing.
  - `PidController`: Feedback-Loop zur dynamischen Anpassung der Kandidatenpool-Größe basierend auf Latenzmessungen. Validiert `measured_latency_ms.is_finite()`; gibt bei NaN/Inf den unveränderten Pool-Wert zurück.
  - `ReplicatorState` (Feature `physio-replicator-dynamics` / F-07): Dynamic Weight Allocation via Multiplicative Weights Update Method (MWUM), erzwingt $\sum w_i = 1.0$.

---

### §3.5 `memfuse-graph` — Layer 2 (CSR Graph Engine & PathRAG)
- **Rolle:** Graphspeicher, Personalized PageRank, PathRAG-Traversierung, synaptische Kantenaktualisierung und Perkulationsprüfungen.
- **Module:** `csr.rs`, `ppr.rs`, `path_rag.rs`, `community.rs`, `cascade.rs`, `consistency_enforcement.rs`, `edge_reinforcement.rs`, `edge_reinforcement_buffer.rs`, `percolation.rs`, `provenance.rs`, `session_dag.rs`.
- **Hauptkomponenten & Types:**
  - `CsrGraph`: High-Performance Compressed Sparse Row Graphdarstellung.
  - `PageRank` / `PprCalculator`: Personalized PageRank Iterationen.
  - `PathRag`: Bidirektionale Dijkstra-Traversierung für Multi-Hop Relational Search.
  - `CommunityStabilityTracker`: Verfolgt Graph-Communities über `stability_cycles_required` aufeinanderfolgende Zyklen für REM-Synthese.
  - `SynapticUpdateBuffer` (Feature `physio-synaptic-edges` / F-03): Sperrfreier `RwLock<DashMap<(EntityId, EntityId), f32>>` Co-Aktivierungspuffer. `flush_buffer_to_csr` wendet Hebbsches Lernen und homeostatische Skalierung ($\sum_j w_{ij} \le W_{\max}$) an.
  - `percolation` Modul (Feature `physio-percolation` / F-11): Berechnung der Perkulations-Gesundheitsmetrik $\phi(t) = \frac{\text{active\_edges}}{N \cdot \ln(N)}$ (gibt `None` für $N < 10$ zurück) und automatisches Re-Bonding unverbundener Knotenpaare oberhalb der `rebonding_similarity`.
  - `SessionDag`: Deadlock-freie Lock-Orchestrierung mittels compile-zeitlich geprüfter `NodesGuard`.

---

### §3.6 `memfuse-crypto` (Package-Name: `memfuse-security`) — Layer 2 (Kryptographie & Datenschutz)
- **Rolle:** Verschlüsselung, WAL-HMAC-Verifizierung, DSGVO Art. 17 Löschnachweise und verschlüsselter KV-Cache.
- **Module:** `crypto.rs`, `wal_crypto.rs`, `deletion_proof.rs`, `anti_tamper.rs`, `kv_cipher.rs`, `kv_segment/` (`store.rs`, `segment.rs`, `eviction_worker.rs`).
- **Hauptkomponenten & Types:**
  - `KeyManager`: AES-256-GCM und ChaCha20-Poly1305 Schlüsselverwaltung und Rotation.
  - `IntegrityVerifier`: WAL HMAC-Ketten-Verifizierung. Exponiert `set_last_hmac` und `last_hmac_snapshot` zur Zustandserhaltung bei Replay-Fallbacks.
  - `DeletionProof`: Kryptographischer Nachweis der Datenlöschung (DSGVO Art. 17) via BLAKE3 / HMAC Signaturkette.
  - `KvSegmentStore`: Verschlüsselter, mandantenisolierter KV-Cache für Inferenz-Prefill-Bypass mit LRU-Eviction. Verwendet Ping-Pong `tokio::sync::Notify` Handshake (`notify_batch_released` / `notify_read_complete`) in Evictions-Tests zur rennbedingungsfreien Sperren-Verifizierung.

---

### §3.7 `memfuse-text` — Layer 2 (Textanalyse & BM25 Volltext-Index)
- **Rolle:** Tokenisierung, deutsche Komposita-Zerlegung, Invertierte Indizes und BM25-Scoring.
- **Module:** `bm25.rs`, `inverted.rs`, `morphology.rs`, `tokenizer.rs`.
- **Hauptkomponenten & Types:**
  - `Bm25Index`: Invertierter Index mit BM25-Termgewichtung ($k_1=1.2, b=0.75$) und TF-IDF-Normalisierung.
  - `GermanCompoundSplitter` / `Morphology`: Zerlegung deutscher Zusammensetzungen (z. B. "Donaudampfschifffahrt" -> ["Donau", "Dampf", "Schiff", "Fahrt"]).
  - `Tokenizer`: Unicode-Word-Boundary-Tokenisierung mit Stopwort-Filterung.

---

### §3.8 `memfuse-candle` — Layer 3 (Air-Gap Candle GGUF Engine & Grounding)
- **Rolle:** Pure Rust Inferenz-, Embedding- und Grounding-Backend ohne externe Netz- oder Prozesstrennwand.
- **Module:** `embedding.rs`, `embedding_provider.rs`, `gasp.rs`, `gguf_loader.rs`, `inference.rs`, `kv_bridge.rs`, `model_registry.rs`.
- **Hauptkomponenten & Types:**
  - `CandleEmbedClient`: Implementiert `EmbeddingProvider`. Erzwingt ein striktes Batch-Limit von `MAX_CANDLE_EMBED_BATCH_SIZE = 256` (wirft `EmbeddingError::Unavailable` bei Überschreitung).
  - `CandleInferenceEngine`: GGUF Local Model Loader und Inferenz-Engine.
  - `GaspValidator`: Implementiert `ResponseGroundingValidator` Trait zur Berechnung des Halluzinations-Scores ($[0.0, 1.0]$).
  - `KvBridge`: Verbindet `CandleInferenceEngine` mit `KvSegmentStore`.

---

### §3.9 `memfuse-index` — Layer 3 (Vektorindex Engine & Quantisierung)
- **Rolle:** HNSW-Vektorindex, SQ8-Quantisierung, DiskANN-Speicherung und Nukleations-Recall-Verfolgung.
- **Module:** `hnsw.rs`, `distance.rs`, `quantize.rs`, `diskann.rs`, `partial_rebuild.rs`, `persistence.rs`.
- **Hauptkomponenten & Types:**
  - `HnswIndex`: Hierarchical Navigable Small World Graph mit Cosine, Euclidean und Dot-Product Distanzmaßen.
  - `ScalarQuantizer`: SQ8 8-Bit-Quantisierung mit SIMD-beschleunigter Distanzberechnung.
  - `DiskAnnIndex`: Disk-backed Vektorindex für vergrößerte Datenmengen.
  - `PartialRebuild`: Reorganisiert den HNSW-Graph partiell unter Beibehaltung der Nucleation Recall Stabilität (kontrolliert über CI Toleranzband 0.05).

---

### §3.10 `memfuse-ollama` — Layer 3 (Ollama Integration & Query Rewriter)
- **Rolle:** REST API Integration für Ollama LLM Services & Iteratives Abfrage-Rewriting.
- **Module:** `client.rs`, `embedding.rs`, `importance.rs`, `model_info.rs`, `context_prefixer.rs`.
- **Hauptkomponenten & Types:**
  - `OllamaClient`: Implementiert `LlmTextGenerator` sowie `memfuse_db::QueryRewriter` via `OllamaClient::generate_text` für mehrstufige iteratives Query-Expansion.
  - `OllamaEmbedding`: Externe Embedding-Generierung über Ollama Endpunkte.

---

### §3.11 `memfuse-embed` — Layer 4 (ONNX Embeddings & Cross-Encoder Reranker)
- **Rolle:** High-Performance Embeddings und Neural Reranking.
- **Module:** `lib.rs`, `reranker.rs`.
- **Hauptkomponenten & Types:**
  - `TextEmbedder`: ONNX / FastEmbed Client mit `MAX_EMBED_BATCH_SIZE = 512`. Exponiert `TextEmbedderConfig.max_batch_size`.
  - `CrossEncoderReranker` (Feature-gated via `#[cfg(feature = "onnx")]`): Cross-Encoder Reranking mit implizitem Feedback (`record_implicit_feedback`), Platt-Scaler Kalibrierung (`is_calibrated()`, `calibration_observation_count()`).

---

### §3.12 `memfuse-store` — Layer 3 (LSM-Tree Storage Engine & WAL)
- **Rolle:** Persistente, crash-sichere Storage-Engine mit LSM-Tree Architecture, Memtable, SSTables, Bloom-Filtern und WAL-Integrität.
- **Module:** `lsm.rs`, `memtable.rs`, `sstable.rs`, `wal.rs`, `manifest.rs`, `compaction.rs`, `mmap.rs`, `checkpoint.rs`, `system_pressure.rs`, `tenant_codec.rs`, `util.rs`.
- **Hauptkomponenten & Types:**
  - `LsmStorage`: LSM-Engine Orchestrator (`LsmConfig`, `WalConfig` setzen `min_wal_version` standardmäßig auf `WalVersion::V2`). Replay stuft legacy V1/V2 WALs direkt auf V3 um.
  - `Wal`: Append-Only Log mit HMAC-Sicherung.
    - *Snapshot-Restore-Muster:* `Wal::prepare_batch()` gibt `(Vec<WalEntry>, [u8; 32])` zurück. Schlägt `append_batch()` in `LsmStorage::commit()` fehl, setzt `Wal::restore_last_hmac(prev_hmac_snapshot)` den HMAC-Zustand zurück, um HMAC-Divergenz zu verhindern.
    - *Recovery:* `recover_from_bak_if_present` stellt `.v1.bak` oder `.v2.bak` wieder her, falls das Haupt-WAL 0 Bytes groß ist. Unverschlüsselte V1-Einträge werden bei aktivem `KeyManager` verweigert.
  - `MemTable`: Lock-freie SkipList / BTreeMap Staging-Struktur.
  - `SSTable`: Blockbasierte SSTable-Dateien mit Bloom-Filter und CRC32-Fast Checksums. Chaos-Bit-Flip-geprüft.
  - *Rollback Invariant:* `MIN_ENTRIES_FOR_SSTABLE_REBUILD = 8`. Überlebende Einträge $<8$ aus einer SSTable werden beim Transaktions-Rollback (`rollback_to_tx_locked`) direkt in den Memtable eingefügt.

---

### §3.13 `memfuse-db` — Layer 5 (Haupt-Orchestrierungs-Engine)
- **Rolle:** Zentraler Datenbank-Orchestrierer. Vereint CRUD, Multi-Signal Hybrid Search, Resonanz-Fusion, Sleep Cycle Maintenance, REM Konsolidierung und Export/Import.
- **Lock-Hierarchie:**
  1. `MemFuse::collections` (`tokio::sync::RwLock`) ->
  2. `MemFuse::embedder` (`parking_lot::RwLock`) ->
  3. `Collection::insert_lock` (`tokio::sync::Mutex`) / `Collection::embedder` (`parking_lot::RwLock`).

- **Detaillierte Modulübersicht:**
  - `collection/` (`mod.rs`, `crud.rs`, `search.rs`, `relate.rs`, `tx.rs`, `kv_lock.rs`, `maintenance.rs`, `query_builder.rs`): `Collection<S: StorageEngine, V: VectorIndex>` ist die generische Kerndatenstruktur. Methoden: `insert()`, `get()`, `update()`, `upsert()`, `delete()`, `search()`, `hybrid_search()`, `relate()`, `scan_prefix()`, `scan()`, `insert_many()`, `upsert_many()`, `set_embedder()`, `community_detection_trigger_threshold()`.
  - `fusion.rs`: RRF-Fusion für bis zu 5 Signale. `build_provenance`, `weighted_reciprocal_rank_fusion_with_options` erlauben $rrf\_k \ge 0.0$. `f32::total_cmp`-Guards gegen NaN-Propagation.
    - *Invariant INV-PROV-1:* Synaptisches Signal wird als 5. Signal (`SignalKind::Synaptic`) erfasst.
    - *Invariant INV-PROV-2:* Resonanz-Bonus $\gamma \cdot (S/T)^\beta$ verändert nicht `signal_contributions`, sondern wird separat in `provenance.coherence_bonus` verbucht.
  - `multistep.rs`: Multi-Step Query Engine für iteratives Query-Rewriting (bis zu 3 Runden) mit LLM-agnostischem `QueryRewriter`-Trait.
  - `chunker.rs`: `MarkdownChunker` für semantische Markdown-Zerlegung mit Breadcrumb-Metadaten und Heading-Hierarchie (~512 Token Zielgröße).
  - `context.rs` / `context_compaction.rs`: `ContextCompactor::consolidate_via_llm` erzeugt `ProvenanceRecord::synthesized_from(&source_doc_ids)` in Metadata `"provenance"`.
  - `temporal_filter.rs`: Bi-temporale Gültigkeitsfenster-Evaluierung.
  - `filter.rs`: Metadaten-Filter-Ausführung (Signal 4).
  - `decay_controller.rs`: `DecayController` für adaptiven Relevanzzerfall.
  - `homeostat.rs`: PID-Homöostase-Regelung (konsolidiert in `memfuse_calibration::PidController`).
  - `memory_consolidation.rs`: **Structural Consolidation Pass** (deterministisch, LLM-frei). Sliding-Window-Clustering zeitlich benachbarer Turn-Embeddings, Near-Duplicate Detection und Identifikation verwaister Graphkanten. Typen: `ConsolidationConfig`, `TurnSegment`, `ConsolidationPhaseResult`, `CommunityStabilityTracker`.
  - `synthesis_phase.rs`: **Generative Synthesis Pass** (LLM-basiert via `run_structural_synthesis_pass`). Synthetisiert stable Communities aus `CommunityStabilityTracker` zu `MetaChunk`s ("`[SYNTHESIZED FROM {n} SOURCES]`"). Gefiltert nach `SynthesisConfig.min_grounding_score`.
  - `consolidation_executor.rs`: Verbindet Consolidation-Outputs mit der Collection-Mutation-API (Tombstones, Graph-Cascade via `memfuse_graph::cascade_invalidate_edges_for_superseded_doc`).
  - `maintenance_scheduler.rs` / `maintenance_config.rs`: `PhysioScheduler` (konfiguriert über `PhysioConfig` in `physio_config.rs`) konsolidiert Hintergrundaufgaben (Thermostat-Eviction, Percolation-Checks bei 0 aktiven Agent-Sessions, MWUM Replicator Dynamics, NREM Sleep Cycle) in einen einzigen periodischen Tick (Standard: 60s) mit WAL Intent Tracking (`__physio_intent:tick`).
  - `background_workers.rs`: Hintergrund-Task Infrastruktur.
  - `transaction.rs`: `DbTransaction`.
  - `volatile_vault.rs`: In-memory verschlüsselter Tresor für volatile Tool-Outputs.
  - `export.rs` & `import.rs`: Memory-Export/Import v1 Format (`memfuse-export-v1.json`). Liest und schreibt Dokumente, Roh-Embeddings, `importance_score` und Graph-Beziehungen idempotent wieder ein.
  - `scan_prefix` / `scan`: Erzwingt `MAX_SCAN_RESULTS_DEFAULT = 10_000` zum OOM-Schutz.

---

### §3.14 `memfuse-router` — Layer 6 (Conformal Prediction & Drift Surveillance)
- **Rolle:** Dynamisches Abfrage-Routing über Konforme Vorhersage und Lyapunov-Drift-Überwachung.
- **Module:** `router.rs`, `profile.rs`, `lyapunov.rs`, `dispatch.rs`, `outcome.rs`.
- **Hauptkomponenten & Types:**
  - `RouterEngine`: Evaluierte Routing-Profile via `select_profile_cascade`. Benötigt `st.conformal.window_total >= 50` für Aktivierung der konformen Score-Kalibrierung (`is_calibrated = true`). Basiert Kandidaten-Scoring per ADR-059 ausschließlich auf `&effective_profiles` (`compute_profile_scores`).
  - `LyapunovDriftWatcher`: Berechnet zeitabhängige Lyapunov-Exponenten über Abfrage-Trajektorien zur Abweichungserkennung. Exponiert `record_outcome(decision_id, outcome) -> bool`.

---

### §3.15 `memfuse-agent` — Layer 7 (Agent Tool Execution & Budget Orchestrations)
- **Rolle:** Agenten-Workflow Engine mit atomarer Budget-Reservierung, Dead-Letter-Queue und Audit-Tracing.
- **Module:** `engine.rs`, `step.rs`, `context.rs`, `graph.rs`, `audit.rs`, `dlq.rs`, `event_source.rs`.
- **Hauptkomponenten & Types:**
  - `OrchestratorEngine`: Führt Agenten-Schritte als Zustandsautomat aus.
  - `AgentTool` Trait: Definiert `fn estimated_cost(&self, input: &Value) -> usize` (Standard: 0). Wird von der `OrchestratorEngine` zur strikten prä-exekutiven atomaren Budget-Reservierung genutzt.
  - `AuditLogger`: Protokolliert Ausführungsschritte prozessisoliert.
  - `DeadLetterQueue`: Erfasst fehlgeschlagene Ausführungsschritte zur Wiederherstellung.

---

### §3.16 `memfuse-mcp` — Layer 8 (MCP Server & Zero-Trust Sandbox)
- **Rolle:** Primäre externe Schnittstelle über das Model Context Protocol (MCP) über stdio/HTTP JSON-RPC 2.0.
- **Module:** `bin/memfuse-mcp-server.rs`, `protocol.rs`, `config.rs`, `prompt_injection.rs`, `sandbox.rs`.
- **MCP Tool Parameterstrukturen:**
  1. `memfuse_search`: Eingabe `query` (string, required), `collection` (string, default `"default"`), `k` (integer, default `10`). Liefert Suchtreffer in `<untrusted_context>` Tags.
  2. `memfuse_insert`: Eingabe `id` (string), `text` (string, required), `collection` (string), `metadata` (object). Auto-Chunking via `MarkdownChunker` (~512 Token Target).
  3. `memfuse_get`: Eingabe `id` (string, required), `collection` (string).
  4. `memfuse_collections`: Liefert Array aller Sammlungen.
  5. `memfuse_consolidate`: Manueller synchroner Admin-Trigger. Eingabe `collection` (string, default `"default"`). Liefert `turns_scanned`, `segments_created`, `duplicates_tombstoned_count`, `cascade_tombstones_count`, `synthesized_count`.
- **Sicherheitskomponenten:**
  - `PromptInjectionGuard`: NFKC-Unicode-Normalisierung, Removal von Zero-Width-Zeichen, rekursive Base64-Payload-Dekodierung bis Tiefe 2.
  - `VolatileSandbox`: AES-256-GCM-SIV RAM-Verschlüsselung via `memfuse-crypto::kv_segment` (Zeroize-on-Drop).

---

### §3.17 `memfuse-py` — Grenzschicht (Isoliertes FFI Workspace)
- **Rolle:** PyO3 CPython FFI-Bindings für Python (`pip install memfuse`). Ist als unabhängiges Workspace entkoppelt (`crates/memfuse-py`).
- **Module:** `src/lib.rs`, `python/memfuse/client.py`.
- **Hauptkomponenten & API:**
  - `open(path, dimension=768, max_elements=None, encryption_passphrase=None, distance_metric=None)` -> `PyMemFuse`. Validiert `dimension` (1–10.000). Default `dimension=768` ist konsistent mit `MemFuseConfig::default().dimension`.
  - `PyMemFuse`: Facade für `collection(name)` -> `PyCollection`, `list_collections()`, `flush()`, `stats()` -> `PyDbStats`.
  - `PyCollection`: `stats()`, `len()`, `is_empty()` sowie CRUD-Methoden via `memfuse_crud_methods!` Macro (`insert`, `get`, `update`, `upsert`, `delete`, `search`, `search_fb`, `hybrid_search`, `hybrid_search_fb`, `relate`, `scan_prefix`, `scan`).
  - `PyDbStats`: Exponiert `index_stats`, `storage_stats`, `drift_status`, `calibration_ece`, `last_calibration_at`, `pid_pool_size`.
- **GIL & Runtime Safety:**
  - `run_blocking_ffi`: Gibt GIL via `py.allow_threads()` frei und fängt Rust-Panics an FFI-Grenze mit `std::panic::catch_unwind` in `PyRuntimeError` ab (`panic = "unwind"` in Cargo.toml).
  - PEP 684 Sub-Interpreter Isolation: Eigene `PyRuntimeState` Tokio-Runtime pro CPython Sub-Interpreter.

---

### §3.18 `memfuse-bench` & `xtask` — Evaluation & Automated Developer Tooling
- **`memfuse-bench`:**
  - LongMemEval (`long_mem_eval.rs`) und LoCoMo (`locomo.rs`).
  - `LongMemEvalCase` nutzt `pub answer: serde_json::Value` mit `answer_str()` zum sicheren Parsen heterogener JSON-Formate.
  - Baseline CI-Gate: `longmemeval_s`.
- **`xtask` Subcommands:**
  - `cargo xtask claim`: Crate-Claiming mit GitHub-Issue Integration (`claimed`, `claim:<crate>`) und Fallback zu `.jules/claims.json`. Verwaltet TTL (4h Standard).
  - `cargo xtask jules-preflight`: Verifiziert Konfliktfreiheit aktiver Claims via `check_no_active_claim_conflict`.
  - `cargo xtask check-recall-stability`: Vergleicht 30-Tage HNSW Nucleation Recall Historie mit `RECALL_TOLERANCE_BAND = 0.05`.
  - `cargo xtask check-duplicate-symbols`: Fast Intra-File Symbol Check und optional `--cross-module` crate-weite Prüfung auf doppelte öffentliche `fn` / `async fn` Deklarationen.
  - `cargo xtask mutation-score-record`: Aktualisiert Mutation-Score-Metriken in `docs/mutation_score_history.jsonl`.
  - `cargo xtask check-jules-context-freshness`: Validiert den `Stand:` Timestamp in `JULES_CONTEXT.md` gegen `DECISIONS.md` und `WORKING_STATE.md`.

---

## §4 — Systemweite Datenflüsse & Lebenszyklen

```
                               ┌────────────────────────────────────────────────────────┐
                               │                    AGENT / MCP CLIENT                  │
                               └───────────────────────────┬────────────────────────────┘
                                                           │ JSON-RPC / PyO3 / Native
                                                           ▼
                               ┌────────────────────────────────────────────────────────┐
                               │             memfuse-mcp / memfuse-py / API             │
                               └───────────────────────────┬────────────────────────────┘
                                                           │ Prompt Guard / Budget Check
                                                           ▼
                               ┌────────────────────────────────────────────────────────┐
                               │                      memfuse-db                        │
                               └──────┬────────────────────┬────────────────────┬───────┘
                                      │                    │                    │
                  ┌───────────────────┘                    │                    └───────────────────┐
                  ▼                                        ▼                                        ▼
┌───────────────────────────────────┐    ┌───────────────────────────────────┐    ┌───────────────────────────────────┐
│           SCHREIBPFAD             │    │             LESEPFAD              │    │          KONSOLIDIERUNG           │
├───────────────────────────────────┤    ├───────────────────────────────────┤    ├───────────────────────────────────┤
│ 1. Transaktion öffnen (TxId)      │    │ 1. Query-Rewriting (Ollama/Pass)  │    │ 1. PhysioScheduler Period Ticks   │
│ 2. Document Chunking & Tokenizing │    │ 2. Parallel 5-Signal Search:      │    │ 2. Thermostat Memory Eviction     │
│ 3. Embedding (ONNX/Candle)        │    │    - HNSW Vector Index            │    │ 3. Percolation Graph Check        │
│ 4. MemTable Insert + WAL Append   │    │    - BM25 Fulltext Index          │    │ 4. MWUM Signal Weight Rebalancing │
│ 5. Graph Edge & Entity Insertion  │    │    - CSR Graph PageRank / PathRAG │    │ 5. REM Phase Generative Synthesis │
│ 6. Commit & In-Memory Lock Free   │    │    - Metadata Filtering           │    │    - Community Stability Tracker  │
│    SSTable Flush bei Boundary     │    │    - Synaptic Co-Activation Buffer │    │    - Grounding Score Verification │
└───────────────────────────────────┘    │ 3. RRF Fusion + Resonanz Bonus    │    │    - MetaChunk Generation         │
                                         │ 4. Cross-Encoder Reranking        │    └───────────────────────────────────┘
                                         │ 5. Conformal Concoction / Drift   │
                                         └───────────────────────────────────┘
```

### §4.1 Schreibpfad (`insert` / `upsert`)
1. Transaktionsinitiierung und Vergabe einer monotonen `TxId`.
2. Segmentierung des Dokuments via `memfuse-text` (Tokenisierung und Komposita-Zerlegung).
3. Vektorgenerierung über `memfuse-embed` (ONNX) oder `memfuse-candle` (Air-Gap).
4. Synchrones Schreiben in die `MemTable` und Anhängen des nummerierten `WalEntry` an das WAL mit HMAC-Signatur-Aktualisierung (`prepare_batch`).
5. Einfügen relationaler Entitäten und Kanten in den `CsrGraph`.
6. Transaktions-Commit. Bei MemTable-Sättigung erfolgt der Flush auf SSTables mit Bloom-Filter-Erstellung.

### §4.2 Lesepfad (`hybrid_search` 5-Signal-Fusion)
1. Eingang der `SearchQuery` an der `Collection`.
2. Parallele Generierung der 5 Candidate-Signale:
   - Vektorsuche via `HnswIndex` (Cosine / Euclidean Distance).
   - Volltextsuche via `Bm25Index`.
   - Relationale Graphsuche via `CsrGraph` (PageRank / PathRAG).
   - Metadaten-Evaluierung via `Filter`.
   - Synaptische Kanten-Aktivierung via `SynapticUpdateBuffer`.
3. Reciprocal Rank Fusion (RRF) Berechnung mit $rrf\_k \ge 0.0$.
4. Anwendung des Resonanz-Bonus $\gamma \cdot (S/T)^\beta$ bei Multi-Signal-Koinzidenz.
5. Cross-Encoder Reranking via `CrossEncoderReranker` mit Rückkopplung an `PlattScaler`.
6. Konforme Score-Prüfung (`RouterEngine`) und Aktualisierung des `LyapunovDriftWatcher`.

### §4.3 Generierungspfad mit KV-Cache-Bridge
1. Bei LLM-Inferenzanfragen im Air-Gap-Modus prüft `CandleInferenceEngine` den verschlüsselten `KvSegmentStore` auf treffende Dokument-Chunks.
2. Bei Cache-Hit werden vorberechnete Key-Value-Tensoren direkt injiziert, wodurch der Prefill-Schritt entfällt.
3. Bei Cache-Miss wird der Prefill berechnet und verschlüsselt im `KvSegmentStore` unter der entsprechenden `TenantId` abgelegt.

### §4.4 Konsolidierungspfad & Biological Sleep Cycle
1. `PhysioScheduler` führt periodisch (alle 60s) Hintergrundprüfungen mit WAL-Intent-Marker `__physio_intent:tick` aus.
2. Thermostat-Eviction entfernt inaktive Chunks basierend auf `ImportanceScore` und Decay.
3. Percolation Check analysiert die Graph-Dichte $\phi(t)$ bei Null aktiven Agenten-Sessions.
4. MWUM Replicator Dynamics berechnet optimale RRF-Signal-Gewichtungen neu.
5. NREM / REM Sleep Cycle:
   - `CommunityStabilityTracker` identifiziert stabile Entitäts-Communities.
   - `run_structural_synthesis_pass` erzeugt synthetisierte `MetaChunk`s ("`[SYNTHESIZED FROM {n} SOURCES]`").
   - Synthesen unterhalb `SynthesisConfig.min_grounding_score` werden verworfen.

### §4.5 Export & Import Lebenszyklus
- Export: `MemoryExporter` serialisiert Sammlungsstrukturen, Dokumente, Embeddings, Wichtigkeits-Scores und Entitäts-Kanten in ein deterministisches v1 JSON-Format.
- Import: `MemoryImporter` liest das JSON-Format ein, stellt Entitäten wieder her und baut HNSW- und BM25-Indizes idempotent auf.

---

## §5 — Sicherheits-, Datenschutz- & Zero-Trust-Modell

1. **WAL Integrity & Non-Repudiation:** WAL-Einträge sind über eine HMAC-SHA256 Kette gebunden. Schlägt ein Batch fehl, stellt `restore_last_hmac` den korrekten Krypto-Zustand wieder her. Unverschlüsselte Legacy-Logs werden abgelehnt, wenn eine `KeyManager` Verschlüsselung aktiv ist.
2. **Kryptographischer Löschnachweis (DSGVO Art. 17):** Löschungen erzeugen unveränderliche `DeletionProof` Objektstrukturen mit BLAKE3 Hash-Verbindungen.
3. **Zero-Trust Input Sanitization:** MCP Tool-Inputs durchlaufen den `PromptInjectionGuard`, um Direct und Indirect Prompt Injections abzuwehren (NFKC Normalierung, Zero-Width stripping, Base64 Tiefe 2).
4. **Volatile Output Vault:** Temporäre Tool-Ergebnisse werden im `VolatileSandbox` / `VolatileVault` verschlüsselt vorgehalten.
5. **Air-Gap Inferenz & Encrypted KV-Cache:** Lokale Candle GGUF-Inferenz vermeidet jeglichen Datenabfluss. KV-Cache-Tensoren im `KvSegmentStore` werden mandantenisoliert mit AES-256-GCM / ChaCha20-Poly1305 verschlüsselt.

---

## §6 — Bekannte Architektur- und Härtungsthemen (Matrix H-1 bis H-16)

| ID | Bereich | Beschreibung | Status / Maßnahme |
|:---|:---|:---|:---:|
| H-1 | `memfuse-store` | WAL-I/O teilweise unter Write-Lock | Monitor latency under extreme load |
| H-2 | `memfuse-store` | CheckpointPin-TOCTOU Fenster unter aggressiver Compaction | Inspected, safe via Snapshot Pins |
| H-3 | `memfuse-db` | Graph Cascade Trigger bei Document Tombstone | Gelöst (CRUD & Consolidation verdrahtet) |
| H-4 | `memfuse-crypto` | Key rotation re-encryption strategy | Standard KeyManager re-keying |
| H-5 | `memfuse-candle` | Backpressure Semaphore analog `memfuse-embed` | Vor breitem Rollout ergänzen |
| H-6 | `memfuse-db` | Symbolnamen `run_structural_synthesis_pass` vs `run_synthesis_pass` | Eindeutig getrennt |
| H-7 | `memfuse-db` | Cargo.toml dev-dependency cleanup | Bereinigt |
| H-8 | `memfuse-py` | Dimension Default Consistency (`open()` 768) | Harmonisierte Defaults (768) |
| H-9 | `memfuse-py` | Version Alignment | Synthetisch synchronisiert |
| H-10 | `memfuse-crypto` | Package Name (`memfuse-security`) | Dokumentiertes Synonym |
| H-11 | `memfuse-py` | Observability Stats Exposure | Gelöst in `PyDbStats` |
| H-12 | `memfuse-db` | Auto Consolidation Trigger | Gelöst via `PhysioScheduler` & `ConsolidationEngine` |
| H-13 | `memfuse-mcp` | Admin Consolidate Tool | Gelöst (`memfuse_consolidate`) |
| H-14 | Test Suite | Nextest function verification | Integrated in CI workflows |
| H-15 | Docs & Context | Context Freshness Verification | Verified via `check-jules-context-freshness` |
| H-16 | Code Comments | Section reference alignment | Standardized to ADR references |

---

## §7 — Betriebsmodi

| Modus | Inferenz-Backend | Embedding-Backend | Storage / Crypto | Anwendungsfall |
|:---|:---|:---|:---|:---|
| **Standard MCP** | Ollama / External | ONNX (`memfuse-embed`) | LSM + WAL HMAC | Default Agent Environment (uvx) |
| **Air-Gap Sovereign** | Candle GGUF Pure Rust | Candle GGUF Embeddings | Encrypted LSM + KV-Bridge | High Security / No Network |
| **Embedded Python** | External / FastEmbed | FastEmbed / ONNX | LSM Embedded | Local Python Agent Pipelines |
| **Native Rust** | Plugged Provider | Native HNSW / Bm25 | Memory / LSM Storage | High-throughput Agent Systems |

---

## §8 — Governance, Quality Control & Automation

- **Zwei-Stufen-Entwicklungsprozess:** Striktes 2-Phase Gate System (Phase 1 Code-Prüfung, Phase 2 Quality-Verification).
- **Claim-Workflow:** Subcommand `cargo xtask claim` schützt Modulbereiche über GitHub-Issues (`claim:<crate>`) oder local `.jules/claims.json` mit 4h TTL. `cargo xtask jules-preflight` sperrt parallele Bearbeitungs-Konflikte.
- **Nucleation Recall Regression Gate:** CI testet 30-Tage HNSW Recall Historien gegen `RECALL_TOLERANCE_BAND = 0.05`.
- **Intra- & Cross-Module Duplicate Symbol Scan:** `cargo xtask check-duplicate-symbols --cross-module` verhindert doppelte Funktionssignaturen im Workspace.
- **Mutation Testing Summaries:** Per-Crate Mutationstests werden automatisch in `docs/mutation_score_history.jsonl` aufgezeichnet.

---

## §9 — Präzises Fachglossar & Invarianten-Index

- **`INV-CAL-1`**: `IsotonicCalibrator` gibt `None` zurück, solange die Anzahl der Beobachtungen kleiner als `warmup_required` ist.
- **`INV-CAL-2`**: `IsotonicCalibrator` setzt aufgezeichnete Beobachtungen zurück, sobald sich der `ConfigFingerprint` ändert.
- **`INV-PROV-1`**: Die Aktivierung des synaptischen 5-Signal-Retrievals erzeugt ein Provenance-Signal des Typs `SignalKind::Synaptic`.
- **`INV-PROV-2`**: Resonanz-Kohärenz-Boni werden exklusiv im Feld `coherence_bonus` verbucht und verfälschen nicht die Rohbeiträge `signal_contributions`.
- **`PathRAG`**: Bidirektionale Graph-Traversierung auf Basis des Dijkstra-Algorithmus über CSR-Datenstrukturen zur Ermittlung verknüpfter Wissens-Pfade.
- **`PhysioScheduler`**: Periodischer Hintergrund-Scheduler zur Zusammenfassung aller physiologischen Konsolidierungsaufgaben (Eviction, Percolation, MWUM, Sleep Cycle).
- **`DeletionProof`**: Cryptographic Erasure Proof zur fälschungssicheren Verifizierung der Datenlöschung gemäß DSGVO Art. 17.
