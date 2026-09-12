# MemFuse — Gesamtspezifikation des Endprodukts

> **Status:** Verbindlich · Einzige normative Quelle für Produkt, Architektur und Roadmap
> **Stand:** 2026-09-12 · Live-verifiziert gegen `HEAD dabdc6317455a9e8111321dd883351eacc7f5b8d` (`https://github.com/tfufuz1/memfuse`, Commit #2191)
> **Codeumfang:** > 150.000 Zeilen Rust (Crates + Benchmarks + xtask) · 17 Workspace-Crates im Hauptworkspace + `memfuse-py` als isoliertes FFI-Workspace · 0 Git-Tags · 0 veröffentlichte Releases
> **Dieses Dokument steht für sich allein.** Es setzt keine Kenntnis von Vorgängerdokumenten, Abweichungsanalysen oder Fragebögen voraus und enthält keine Verweise auf externe Dateien. Alle Aussagen sind entweder (a) direkt am Quellcode des genannten Repository-Stands verifiziert oder (b) hier getroffene, verbindliche Produktentscheidungen.

---

## §1 — Kernthese

**MemFuse hat die technisch fortschrittlichste Retrieval-Architektur unter allen lokal betriebenen AI-Memory-Systemen — und ihr größtes Risiko ist Unsichtbarkeit, nicht Technologie.**

Fast 150.000 Zeilen produktionsreifer Rust-Code, eine 4-Signal-Retrieval-Fusion mit PathRAG, ein proaktiver Drift-Watcher und eine kryptographisch integritätsgesicherte Storage-Engine existieren — aber es gibt keinen einzigen Git-Tag und kein einziges veröffentlichtes Artefakt. Der oberste Grundsatz dieser Spezifikation:

> **Release schlägt Feature.** Solange kein erstes Alpha-Release existiert, hat jede Aufgabe, die direkt zu einem `uvx`- oder `pip install`-fähigen Artefakt führt, Vorrang vor jeder neuen Retrieval- oder Konsolidierungsarbeit — mit der einzigen Ausnahme von Fehlern mit Silent-Data-Corruption-Risiko.

Der Zustand hat sich seit den letzten zwölf Commits deutlich in Richtung Release bewegt: `memfuse-tauri` ist entfernt, ONNX ist Default-Embedding-Backend, `memfuse-mcp` ist `uvx`-paketierbar, `memfuse-candle` ist als echter Air-Gap-Pfad integriert, ein Memory-Export-Format v1 existiert, ein `LlmTextGeneratorStreaming`-Trait ist implementiert, die Generative-Synthesis-Konsolidierung läuft nun als eigenständiger `ConsolidationEngine`-Hintergrund-Task, und Drift-/Kalibrierungsmetriken sind bis in die Python-Grenzschicht exponiert. Der verbleibende Weg zum Release ist kurz und in §11 exakt benannt — er reduziert sich im Wesentlichen auf zwei Namens-/Versions-Inkonsistenzen, ein fehlendes MCP-Tool und Code-Hygiene-Aufräumarbeiten.

---

## §2 — Produktvision

### §2.1 Was MemFuse ist

MemFuse ist eine souveräne, lokal betriebene Gedächtnisschicht für KI-Agenten. Sie wird primär als **MCP-Server** (`memfuse-mcp`, Installation via `uvx`) und sekundär als **Python-Library** (`pip install memfuse`) sowie als **Rust-Crate** (`crates.io`) verteilt. MemFuse ist reine Infrastruktur — kein menschlicher Endnutzer interagiert je direkt mit MemFuse ohne einen dazwischenliegenden Agenten, Assistenten oder ein Framework.

### §2.2 Primärer Eingang: MCP-Server zuerst

1. **`memfuse-mcp`** (jetzt, `uvx`-paketiert) — MCP-Server für Claude Desktop, Cursor, Windsurf, Cline. Installationsfluss: `uvx memfuse-mcp --db-path ~/.memfuse`.
2. **`memfuse` (Python-Library)** (parallel) — `pip install memfuse` für Python-Entwickler, die LangChain/LlamaIndex/eigene Agentenloops mit lokalem Gedächtnis versorgen.
3. **`memfuse` (Rust-Crate)** (Nebenprodukt) — `crates.io`-Distribution für Rust-native Agentenframeworks.
4. **`memfuse` Enterprise** (Fernziel) — DSGVO-konforme, auditierbare Deployments, relevant sobald 1. und 2. echte Nutzer haben.

`memfuse-tauri` (Desktop-App) ist **physisch aus dem Workspace entfernt** (siehe §11) und wird nicht weiterverfolgt.

### §2.3 Alleinstellungsmerkmale

1. **4-Signal-Retrieval-Fusion inkl. PathRAG:** HNSW (Vektor) + BM25 (Volltext, deutsche Komposita-Dekomposition) + CSR-Graph (PageRank) + Metadaten-Filter, fusioniert via Reciprocal Rank Fusion (RRF) mit optionalem Resonanz-Kohärenz-Bonus. PathRAG (bidirektionaler Dijkstra) liefert ein viertes, multi-hop-fähiges Signal.
2. **Kalibriertes Retrieval mit proaktiver Drift-Erkennung:** Isotonic-Kalibrierung (PAVA) + Lyapunov-Drift-Watcher erkennen Qualitätsverschlechterung, bevor sie beim Nutzer sichtbar wird.
3. **MCP-native mit Zero-Trust-Sandbox:** Prompt Injection Guard und volatile Tool-Output-Verschlüsselung sind in `memfuse-mcp` first-class.
4. **Kryptographische Integrität & Löschung:** WAL-HMAC-Kette und `DeletionProof` (kryptographischer Löschnachweis, DSGVO Art. 17) auf Storage-Ebene.
5. **Typsichere Lock-Infrastruktur:** Session-DAG mit `NodesGuard`-Typ erzwingt Lock-Reihenfolge zur Compile-Zeit.
6. **Echter Air-Gap-Modus:** Native Candle-GGUF-Inferenz (Pure Rust) ist als vollwertiger Embedding-Backend-Pfad verdrahtet — kein Ollama-Prozess und kein Netzwerkzugriff nötig.
7. **Portables, versioniertes Memory-Export-Format:** Vollständiger Export einer Collection (Dokumente, Embeddings, Wichtigkeits-Scores, Beziehungen) als JSON, tool- und plattformunabhängig lesbar.

### §2.4 Nicht-Ziele (verbindlich)

- Kein Cloud-SaaS, auch nicht optional gehostet.
- Kein Multi-Tenant-Enterprise-Produkt für gleichzeitige Fremdkunden auf einer Instanz (`TenantId` dient ausschließlich Prozess-/Test-Isolation).
- Kein eigenes LLM-Training/Fine-Tuning-Feature.
- Keine grafische Desktop-Oberfläche.
- Kein eigenes Agentenframework — reine Gedächtnisschicht für externe Frameworks.
- Keine Cloud-Vektordatenbank-Alternative — embedded/lokal-only.
- Kein Voice-Assistant-Interface.

---

## §3 — Architekturprinzipien P1–P20

- **P1 — DAG-Integrität:** `cargo xtask check-dag` ist zwingendes CI-Gate. Kein Code in Layer N darf Abhängigkeiten auf Layer >N besitzen.
- **P2 — Zero-Panic-Doctrine:** Production-Code ist panic-frei. `unsafe` ist beschränkt auf drei funktionale Kategorien: (a) SIMD-Distanzberechnung (`memfuse-index::distance.rs`), (b) Mmap-Persistenz (`memfuse-index::diskann.rs`/`persistence.rs`), (c) plattformspezifische Systemaufrufe mit dokumentiertem `// SAFETY:`-Proof (u. a. `mlock`/`munlock` in `memfuse-db::volatile_vault.rs` sowie eine dokumentierte Windows-ACL-Ausnahme in `memfuse-store::wal.rs`). `memfuse-py` nutzt ein eigenes Workspace-Profil (`panic = "unwind"`) mit `catch_unwind`-Isolation.
- **P3 — WAL-First:** Kein Datenschreibvorgang ohne vorherigen WAL-Commit. `fsync` auf Datei- und Directory-Ebene.
- **P4 — Inferenz-Backend-Agnostizismus:** `LlmTextGenerator` und `TextEmbeddingEngine` in `memfuse-core` sichern die Abstraktion über Ollama, ONNX und Candle hinweg.
- **P5 — Kein Cloud-Zwang:** Inferenz und Retrieval laufen vollständig lokal auf Nutzer-Hardware.
- **P6 — Eine Quelle für Architekturentscheidungen:** `DECISIONS.md` ist die einzige Quelle für ADRs; Code-Kommentare dürfen keine parallelen, veralteten Dokument-Nummerierungen referenzieren.
- **P7 — Code-Nachweis-Pflicht für Marketing-Aussagen:** Quantitative Leistungsversprechen benötigen reproduzierbare Benchmark-Nachweise in `memfuse-bench` — null Ausnahmen.
- **P8 — Kalibrierungs-Integrität:** Jede Änderung an `prompt_template_hash`, `temperature_bits` oder `quantization` invalidiert automatisch alle Kalibrierungsstatistiken (`ConfigFingerprint`).
- **P9 — Kein Klartext-Sensitivspeicher:** Sensitiver Tensor-Speicher wird nach Nutzung via `ZeroizeOnDrop` überschrieben.
- **P10 — Reuse vor Neubau:** Prüfung gegen `TYPE_REGISTRY.md` und CI-Gate `check-duplicate-symbols` (inkl. crate-übergreifender Prüfung) vor Neuanlage von Typen/Modulen.
- **P11 — Latenzbudget-Pflicht für Hot-Path:** `RerankPidController` begrenzt P95-Retrieval-Latenzen.
- **P12 — Physio-Feature-Default-Unsichtbarkeit:** Alle `physio-*`-Feature-Aliase sind per Feature-Flag deaktivierbar, Defaults bleiben transparent.
- **P13 — Modulgrenzen nach Verantwortung:** Klare Trennung Layer 0 (Fundament) bis Layer 8 (Agenten/Interfaces).
- **P14 — Ein Scheduler pro Subsystem:** Konsolidierung in `MaintenanceScheduler`.
- **P15 — Eine Vision pro Release:** MCP-Server zuerst, Python-Library parallel.
- **P16 — Dokumente als Zieldefinitionen:** Dokumente beschreiben Soll-Zustände und maschinenlesbare Invarianten, keine ephemeren Code-Zeilen.
- **P17 — Ambient-Kontext ist keine Garantie:** `AGENTS.md` wird nicht automatisch geladen; jeder Trigger-Prompt erzwingt das Einlesen von `AGENTS.md` und `.jules/SESSION_BOOTSTRAP.md`.
- **P18 — Parallele Sessions brauchen Claims:** `cargo xtask claim --crate X --issue Y` sperrt Ziel-Crates vor paralleler Bearbeitung; ein `--release`-Flag sowie automatische TTL-Expiry (4h) sind implementiert.
- **P19 — Beschlossener, nicht umgesetzter Governance-Beschluss ist gefährlicher als keiner:** Beschlüsse werden per CI-Gate durchgesetzt oder formal widerrufen.
- **P20 — Release schlägt Feature:** Höchste Priorität bei Konflikt (siehe §1).

---

## §4 — Crate-Spezifikation (mikrofein)

Diese Sektion spezifiziert jedes der 17 Hauptworkspace-Crates plus `memfuse-py` einzeln: Zweck, Position im DAG, Abhängigkeiten, Modulstruktur, öffentliche Kernschnittstellen, Feature-Flags und Status.

### §4.1 `memfuse-core-ipc-gen` — Layer 0

Auto-generierter FlatBuffers-IPC-Code für die MemFuse-Kernprotokolle. Keine internen Abhängigkeiten (Wurzel des DAG). Einzelnes generiertes Modul mit FlatBuffers-Typen für Query-/Response-Strukturen, die über die Python- und MCP-Grenzschicht transportiert werden (`search_fb`/`hybrid_search_fb` in `memfuse-py`). Wird nicht manuell editiert; Regeneration via Build-Skript aus `.fbs`-Quellschemata. **Status:** ✅ stabil.

### §4.2 `memfuse-core` — Layer 1 (Fundament, Dependency-Root)

Stellt alle domänenweiten Typen, Trait-Abstraktionen und die einheitliche Fehlerbehandlung bereit. Jedes andere Crate hängt transitiv davon ab. Abhängigkeit: `memfuse-core-ipc-gen`.

**Module:**
- `error.rs`/`error_dto.rs` — `MemFuseError`-Enum, DTO-Serialisierung für Grenzschichten.
- `ipc/` — JSON-RPC-Hilfstypen, von `memfuse-mcp` genutzt.
- `seq_log.rs` — Sequenz-Logging-Primitive für MVCC-Ordering.
- `snapshot.rs` — `SnapshotRegistry` für MVCC-Lese-Isolation.
- `traits/mod.rs` — Kern-Traits: `StorageEngine`, `VectorIndex`, `TextIndex`, `GraphIndex`, `CheckpointCoordinator`, `Checkpoint`, `Snapshot`, `TextEmbeddingEngine`, `LlmTextGenerator`, `SegmentSynthesizer`, `DistanceCalculator`, `MemoryLifecycleManager`, `GroundingValidator`, plus Stats-Typen und `ConsolidationAction`-Enum, `GroundingAssessment`.
- `traits/embedding.rs` — `EmbeddingProvider`, `TextGenerator`, `EmbeddingError`, `MockEmbedder`, sowie **`LlmTextGeneratorStreaming`** (Supertrait von `LlmTextGenerator`): definiert `generate_stream()` für inkrementelle, gestreamte LLM-Antworten (Lifetime-gebundener `Stream`/`BoxStream`-Rückgabetyp). Implementiert in `memfuse-candle::inference.rs` und `memfuse-ollama::client.rs` — beide unterstützten Inferenz-Backends liefern damit Streaming-Antworten über eine gemeinsame Abstraktion.
- `tx_buffer.rs` — `TxBuffer`: sharded Transaction-Staging mit Orphan-Reaper.
- `types.rs` + Untermodule — Domänentypen: `TenantId` (`try_new()`-Guard, `TenantId(0)` = `SYSTEM`-reserviert), `CollectionId`, `DocId`, `EntityId`, `TxId` (alle `#[repr(transparent)]` u64-Newtypes), `DistanceMetric`, `Embedding`, `ScoredDocument`, `MemoryLink`/`LinkRelation`, `Entity`, `Edge`, `MemoryType` (Episodic/Semantic/Procedural/Working), `PprConfig`, `ConfigFingerprint`, `TokenBudget`/`BudgetStrategy`/`ResourceBudget`/`ResourceTracker`, `WorkflowState`, `FilterExpr`, `ImportanceScore`, `DecayFunction`, `MemoryImportance`, `GraphTraversalStrategy`, `FusionWeights`, `ContextChunk`, `ContextWindow`, `ScoredEntry`, `HybridQuery`/`HybridQueryBuilder`.

**Feature-Flags:** `test-utils`. **Status:** ✅ Kern, stabil.

### §4.3 `memfuse-calibration` — Layer 2

Score- und Wahrscheinlichkeits-Kalibrierung. Abhängigkeit: `memfuse-core`.

- `isotonic.rs` — `IsotonicCalibrator` via PAVA, Fingerprint-Invalidierung bei Config-Änderungen.
- `platt.rs` — `PlattScaler` für bekannte Sigmoid-Verteilungen.
- `pid.rs` — PID-Reranking-Controller (`RerankPidController`) für Latenzbudget-Enforcement.
- `replicator.rs` — Replicator-Dynamics-Gewichtungslogik. **Produktentscheidung:** Dieses Modul ist zur physischen Entfernung vorgesehen (siehe §6); bis zur Entfernung ist der zugehörige Konfigurations-Default konsequent auf inaktiv zu setzen.

**Status:** ✅ Kern (isotonic/platt/pid), 🗑️ `replicator.rs` zur Entfernung markiert.

### §4.4 `memfuse-checkpoint` — Layer 2

Öffentlich sichtbarer Checkpoint-Subsystem-Einstiegspunkt für Time-Travel und MVCC-basiertes Snapshotting. Explizit getrennt vom internen, crate-privaten Checkpoint-Modul in `memfuse-store` (dort ausschließlich `pub(crate)`, niemals von außen nutzbar). Abhängigkeit: `memfuse-core`.

**Kernschnittstellen:** `CheckpointCoordinator`-Trait-Implementierung; `PersistentCheckpointStore` (Registry, delegiert Persistenz an `StorageEngine`, thread-sicherer Cache via `parking_lot::RwLock`); `CheckpointGuard` (RAII-Guard für automatisches Rollback bei Fehlern). **Status:** ✅ Kern.

### §4.5 `memfuse-graph` — Layer 2

CSR-Graph für Entity-Relation-Traversal (Signal 3 der Fusion) sowie Session-DAG für Konversationsverzweigung. Abhängigkeiten: `memfuse-core`, `memfuse-store`.

- `csr.rs` — `CsrGraph`: Compressed-Sparse-Row-Graph, speichereffiziente BFS-Traversierung mit Score-Decay; implementiert den `GraphIndex`-Trait.
- `ppr.rs` — Personalized PageRank.
- `community.rs` — Community-Detection (Cluster-Grenzenfindung für Consolidation).
- `path_rag.rs` — PathRAG-Engine: bidirektionaler Dijkstra für Multi-Hop-Traversal, Sufficiency-Gate gegen Precision-Kollaps.
- `session_dag.rs` — `SessionBranchTree`: Konversationsverzweigung als persistierter azyklischer Graph. `NodesGuard`-Typ erzwingt Lock-Reihenfolge zur Compile-Zeit (kein Laufzeit-Check) — Alleinstellungsmerkmal gegen Deadlocks.
- `cascade.rs` — Cascading Invalidation: `DocEdgeIndex` tombstont verknüpfte Graph-Kanten beim Invalidieren von Supersedes-Chunks. **Bekannte Lücke:** Der Trigger Supersedes→Graph-Kanten-Tombstone ist noch nicht vollständig verdrahtet und ist vor dem Release zu vervollständigen.
- `consistency_enforcement.rs` — Widerspruchsabwehr zwischen konkurrierenden Fakten (Kernfeature).
- `edge_reinforcement.rs`/`edge_reinforcement_buffer.rs` — Hebbianisches Kanten-Reinforcement (hinter Feature-Flag).
- `percolation.rs` — Graph-Connectivity-Health-Metrik (Perkolationsanalyse).
- `provenance.rs` — Herkunftsnachweis pro Kante/Ergebnis.

**Feature-Flags:** `default = []`, `graph-connectivity-health`, `edge-reinforcement-learning`, sowie rückwärtskompatible Alias-Namen (`physio-percolation` → `graph-connectivity-health`, `physio-synaptic-edges` → `edge-reinforcement-learning`) für `memfuse-db`-Konsumenten, die auf die technische Terminologie migriert werden. **Status:** ✅ Kern, ⏳ Cascade-Trigger-Lücke offen.

### §4.6 `memfuse-crypto` (Package-Name `memfuse-security`) — Layer 2

Verschlüsselung und Integritätsschutz für WAL und SSTables. Abhängigkeit: `memfuse-core`. Namenskonvention: Verzeichnisname `memfuse-crypto`, Cargo-Package-Name `memfuse-security`; beide Namen sind in der Dokumentation als Synonyme zu behandeln.

- `crypto.rs` — AES-256-GCM-SIV-Kernprimitiven, HKDF-Schlüsselableitung (pro Datei ein eindeutiger Schlüssel, Nonce-Reuse-Mitigation).
- `wal_crypto.rs` — WAL-HMAC-Kette (Manipulationsschutz, Anti-Tamper).
- `anti_tamper.rs` — ergänzende Integritätsprüfungen.
- `deletion_proof.rs` — `DeletionProof`: kryptographischer Löschnachweis (DSGVO Art. 17) mit `ExcludedScope`-Deklaration für Fine-Tuning-/LLM-Parametergedächtnis-Ausnahmen.
- `kv_cipher.rs` — Verschlüsselung für Key-Value-Segmente.
- `kv_segment/` — volatiler, verschlüsselter KV-Segment-Store (Grundlage für MCP-Sandbox-Volatile-Vault).
- `error.rs` — crate-lokale Fehlertypen.

**Zentrale Invarianten:** absolut lock-frei und frei von synchronen/asynchronen I/O-Operationen in den kryptographischen Kernpfaden; Zeroize-on-Drop. **Feature-Flags:** `test-utils`, `kv-encryption`. **Status:** ✅ Kern.

### §4.7 `memfuse-text` — Layer 2

BM25-Volltextsuche mit deutscher Kompositum-Dekomposition (Signal 2 der Fusion). Abhängigkeit: `memfuse-core`.

- `bm25.rs` — BM25+-Scoring, integriert nativ in `fusion.rs` (`memfuse-db`).
- `inverted.rs` — invertierter Index.
- `morphology.rs` — deutsche Kompositum-Dekomposition (Alleinstellungsmerkmal gegenüber englischsprachigen Konkurrenzsystemen).
- `tokenizer.rs` — Tokenisierung.

**Status:** ✅ Kern.

### §4.8 `memfuse-candle` — Layer 3

Native Candle-GGUF-ML-Inferenz-Engine (Pure-Rust-Inferenz ohne externe Prozessabhängigkeit) — die technische Grundlage des Air-Gap-Versprechens. Abhängigkeiten: `memfuse-core`, `memfuse-calibration`.

**Architektur-Strategie:** "Strategie B" (mistral.rs-artiger Ansatz) — schnell deploybare, native Inferenz-Engine hinter den bestehenden Traits (`LlmTextGenerator`, `EmbeddingProvider`, `TextEmbeddingEngine`), ohne direkten Attention-Level- oder RoPE-Shift-Zugriff auf KV-Cache-Ebene. "Strategie A" (explizite RoPE-Shift-/KV-Cache-Bridge für mandantenisolierte Cache-Projektionen) bleibt separates Zukunftsvorhaben.

- `gguf_loader.rs` — GGUF-Modell-Loader.
- `inference.rs` — `CandleLlmClient`: Device-Abstraktion (CPU/CUDA/Metal), `swap_model()` für Hot-Reload ohne Neustart.
- `embedding.rs`/`embedding_provider.rs` — Candle-basierte Embedding-Erzeugung.
- `model_registry.rs` — Modell-Verwaltung/-Auswahl.
- `gasp.rs` — GASP-Validator (Grounding-Verifikation gegen Halluzination).

**Feature-Flags:** `default = []`, `candle`, `cuda`, `metal`. **Integrationsstatus:** Als `EmbeddingBackend::Candle { model_dir, quantization }`-Variante in `memfuse-db::MemFuseConfig` verdrahtet und über `memfuse-mcp`s `candle`-Feature erreichbar — kein reines "implementiert, aber nicht verdrahtet" mehr. **Verbleibende Härtungspflicht:** Vor breitem Produktions-Rollout muss ein expliziter Backpressure-Vertrag analog zu `memfuse-embed::max_concurrent_embeddings` ergänzt werden, da Inferenzaufrufe in `inference.rs` aktuell ohne Semaphore-Begrenzung blockierend laufen können. **Status:** ✅ verdrahtet, ⏳ Backpressure-Härtung offen.

### §4.9 `memfuse-index` — Layer 3

HNSW-Vektorindex mit SIMD-beschleunigter Distanzberechnung (Signal 1 der Fusion). Abhängigkeiten: `memfuse-core`, `memfuse-security`.

- `hnsw.rs` — HNSW-Index, 2-Phasen-Copy-on-Write-Rebuild für Zero-Downtime-Updates.
- `distance.rs` — SIMD-beschleunigte Distanzfunktionen (AVX2/SSE4, automatische CPU-Erkennung); Ort mit `unsafe`.
- `diskann.rs` — DiskANN-Basisimplementierung, inkrementeller `persist_delta()`-Pfad mit atomarem Rename und Pending-WAL-Puffer; Ort mit `unsafe`.
- `persistence.rs` — Mmap-basierte Persistenz; Ort mit `unsafe`.
- `quantize.rs` — Scalar-Quantisierung für Speichereffizienz.
- `partial_rebuild.rs` — `rebuild_region()`: **dauerhaft gesperrtes Feature** (siehe unten).

**Feature-Flags:** `default = []`, `experimental-diskann` (Status bleibt "experimental"), `partial-index-rebuild` — **verbindliches Produkt-VETO:** Der aktuelle `rebuild_region()` implementiert reines Tombstone-Pruning ohne Re-Wiring, was zu Grad-Verlust ohne Navigierbarkeits-Wiederherstellung führt. Dieses Flag darf nicht aktiviert werden, bevor `test_partial_rebuild_recall_regression()` über 30 Tage stabil grün ist und ein ADR das Veto explizit revidiert. Ersatzstrategie ist der 2-Phasen-CoW-Rebuild in `hnsw.rs`. **Status:** ✅ Kern (HNSW), ⏳ experimentell (DiskANN), 🚫 gesperrt (Partial Rebuild).

### §4.10 `memfuse-ollama` — Layer 3

HTTP-Client für Ollama als externe Inferenz-/Embedding-Backend-Option. Abhängigkeiten: `memfuse-core`, `memfuse-calibration`, `memfuse-embed`.

- `client.rs` — Ollama-HTTP-Client (crate-privat).
- `embedding.rs` — Embedding-Anfragen gegen Ollama (crate-privat).
- `context_prefixer.rs` — Prefix-Engine für Kontextanreicherung.
- `importance.rs` — Importance-Klassifikation via Ollama-LLM (Kernfunktion `score_importance_batch` — dies ist die tatsächliche Fundstelle der produktiven Wichtigkeitsklassifikation, nicht `memfuse-embed`).
- `model_info.rs` — Modell-Metadaten (`nomic-embed-text` als historischer Standard-Referenzwert).

**Status:** ✅ Kern, weiterhin voll unterstützter Backend-Pfad (nicht mehr Default, aber explizit über `EmbeddingBackend::Ollama` konfigurierbar und für Nutzer mit laufender Ollama-Instanz vorgesehen).

### §4.11 `memfuse-embed` — Layer 4

In-Process-Text-Embeddings über ONNX Runtime — ohne externe Prozessabhängigkeit. Abhängigkeiten: `memfuse-core`, `memfuse-calibration`, `memfuse-candle` (optional).

- `lib.rs` — ONNX-Runtime-Integration (`ort`-Crate) und Tokenisierung (`tokenizers`-Crate); dokumentierter Backpressure-Vertrag: `max_concurrent_embeddings` begrenzt `spawn_blocking`-Aufrufe, sodass Aufrufer Backpressure statt Tokio-Thread-Pool-Erschöpfung erfahren. Dieser Vertrag ist das Referenzmuster, an dem sich `memfuse-candle` (§4.8) orientieren muss.
- `reranker.rs` — Cross-Encoder-Reranking (Post-RRF, ONNX-basiert, optionales Feature).

**Feature-Flags:** `default = []`, `onnx` (aktiviert `ort`, `tokenizers`, `ndarray`), `candle-backend` (optional). **Status:** ✅ Implementiert **und aktiver Release-Default** — `EmbeddingBackend::Onnx { model_name, cache_dir }` ist der Default-Wert von `MemFuseConfig::embedding_backend`.

### §4.12 `memfuse-store` — Layer 3

LSM-Tree-basierte Storage-Engine — das Fundament der Persistenzschicht. Abhängigkeiten: `memfuse-core`, `memfuse-security`.

- `wal.rs` — Write-Ahead-Log v3 mit HMAC-Anti-Tamper-Kette pro Block.
- `memtable.rs` — 16-Shard-MemTable (SkipList-basiert).
- `sstable.rs` — SSTable mit Bloom-Filtern und CRC32-Verifikation.
- `compaction.rs` — Tiered- und Leveled-Compaction.
- `lsm.rs` — LSM-Tree-Orchestrierung, Group-Commit-Batching (Leader/Follower-Pattern), MVCC-Snapshot-Isolation; Lock-Hierarchie im Datei-Header dokumentiert. Der Leader behält seine eigenen Daten (`leader_tx_id`, `leader_wal_entries`, `leader_mem_updates`) lokal statt sie über einen eigenen Oneshot-Kanal zu routen; im Fehlerfall wird `rollback_to_tx_locked()` aufgerufen und jeder Follower explizit und einzeln benachrichtigt ("Every follower sender MUST be notified exactly once, even in double-fault paths").
- `manifest.rs` — Crash-Recovery via Manifest, inklusive `repair_on_open()` für konsistente Wiederherstellung nach Pending-Intent-Abbrüchen.
- `mmap.rs` — Memory-Mapped-I/O-Primitiven.
- `checkpoint.rs` — `pub(crate)`, ausschließlich internes MVCC-Snapshot-Pinning, gekoppelt an `SnapshotRegistry`; strikt getrennt von der öffentlichen Checkpoint-API in `memfuse-checkpoint`.
- `system_pressure.rs` — Systemdruck-/Backpressure-Signale (Disk-Full-Erkennung u. a.).
- `tenant_codec.rs` — Kodierung von `TenantId` in Storage-Keys.
- `util.rs` — Hilfsfunktionen.

**Zentrale Invarianten:** WAL-First — kein Schreibvorgang ohne vorherigen WAL-Commit; `TOMBSTONE_BIT`-Disziplin konsistent vor jedem `max_seq`-Vergleich. **Verbleibende, dokumentierte Härtungsbedarfe:** WAL-I/O teilweise unter Write-Lock ausführbar (Latenzrisiko unter Last), CheckpointPin-TOCTOU-Fenster unter aggressiver Compaction — beide sind Beobachtungs-, keine Korruptionsrisiken und vor GA zu schließen. **Feature-Flags:** `default = []`, `fault-injection` (Chaos-Testing). **Status:** ✅ Kern, robust gegen Group-Commit-Rollback- und Crash-Recovery-Fehler.

### §4.13 `memfuse-db` — Layer 5 (Orchestrierungs-Kern)

Eingebettete Hybrid-Search-Engine für KI-Agenten — der zentrale Orchestrator, der alle Retrieval-Signale, Storage, Graph und Inferenz-Backends zusammenführt. Größtes Crate im Workspace. Abhängigkeiten: `memfuse-core`, `memfuse-crypto`, `memfuse-store`, `memfuse-index`, `memfuse-text`, `memfuse-checkpoint`, `memfuse-graph`, `memfuse-ollama`, `memfuse-calibration`, `memfuse-embed`, `memfuse-candle`.

**Lock-Hierarchie (dokumentiert im `lib.rs`-Header):** 1. `MemFuse::collections` (`tokio::sync::RwLock`) → 2. `MemFuse::embedder` (`parking_lot::RwLock`) → 3. `Collection::insert_lock` (`tokio::sync::Mutex`) / `Collection::embedder` (`parking_lot::RwLock`).

**Module:**
- `collection/` (`mod.rs`, `crud.rs`, `search.rs`, `relate.rs`, `tx.rs`, `kv_lock.rs`, `maintenance.rs`, `query_builder.rs`) — `Collection<S: StorageEngine, V: VectorIndex>`: zentrale Datenstruktur, generisch über Storage- und Index-Implementierung (Default: `LsmStorage`, `HnswIndex`). Kernmethoden: `insert()`, `get()`, `update()`, `upsert()`, `delete()`, `search()`, `hybrid_search()`, `relate()`, `scan_prefix()`, `scan()`, `insert_many()`, `upsert_many()`, `namespaced_key()`, `graph_index()`, `vector_index()`, `storage()`, `load_index()`, `set_embedder()`, `community_detection_trigger_threshold()`.
- `fusion.rs` — 4-Signal-RRF-Fusion (HNSW + BM25 + CSR-Graph + Metadaten-Filter) mit optionalem Resonanz-Kohärenz-Bonus; durchgängige `f32::total_cmp`- und `is_finite()`-Guards härten die Fusion gegen NaN-Propagation.
- `multistep.rs` — Multi-Step Query Engine: iteratives Query-Rewriting bis zu 3 Runden, Abbruch bei konfigurierbarer Qualitätsschwelle, RRF-Fusion über alle Runden hinweg, LLM-agnostischer `QueryRewriter`-Trait.
- `chunker.rs` — `MarkdownChunker`: semantische Zerlegung mit Breadcrumb-Metadaten, Heading-Hierarchie-respektierende Splits, ~512-Token-Zielgröße (vom MCP-Tool `memfuse_insert` genutzt).
- `context.rs`/`context_compaction.rs` — Kontext-Fenster-Verwaltung und -Kompaktierung.
- `temporal_filter.rs` — bi-temporale Filterung (Validity Windows).
- `filter.rs` — Metadaten-Filter-Ausführung (Signal 4 der Fusion).
- `decay_controller.rs` — `DecayController`: adaptiver Zerfall für Retrieval-Relevanz (Kernfeature).
- `homeostat.rs` — PID-Homöostase-Regelung.
- `memory_consolidation.rs` — **Structural Consolidation Pass** (deterministisch, LLM-frei): sequenzielles Sliding-Window-Clustering zeitlich benachbarter Turn-Embeddings, segmentlokale Near-Duplicate-Detection, Identifikation verwaister Graph-Kanten. Bewusst ohne LLM-API-Aufrufe und ohne Abhängigkeit zu `memfuse-graph` innerhalb des Moduls (DAG-Integrität). Zentrale Typen: `ConsolidationConfig` (`min_turns_per_segment`, `max_turns_per_segment`, Cosine-Similarity-Schwelle 0.70 für Segment-Kohäsion), `TurnSegment`, `ConsolidationPhaseResult`, `CommunityStabilityTracker`.
- `synthesis_phase.rs` — **Generative Synthesis Pass** (LLM-basiert, bewusst getrennt vom Structural Pass): synthetisiert pro konsolidiertem Segment einen abstrakten `SynthesizedChunk` via `SegmentSynthesizer`-Trait. Funktion `run_synthesis_pass()` ist vollständig implementiert und aufrufbar.
- `consolidation_executor.rs` — verbindet den Structural-Consolidation-Output mit der Collection-Mutation-API (Tombstones, Graph-Cascade-Anwendung via `memfuse_graph::cascade_invalidate_edges_for_superseded_doc`, siehe §4.5); ruft `run_consolidation_pass()` und `run_synthesis_pass()` auf. Die Cascade-Invalidierung ist hier **und** im regulären CRUD-Schreibpfad (`collection/crud.rs`) automatisch verdrahtet — jedes Supersedes-Ereignis tombstont beim Schreiben wie bei der Konsolidierung zuverlässig die abgeleiteten Graph-Kanten.
- `maintenance_scheduler.rs` — `MaintenanceScheduler`: konsolidierte Ausführung von Reclaim-, Cleanup- und Consolidation-Tasks. Der Structural Consolidation Pass ist über das Flag `background_consolidation_enabled` (Default: `false`) und einen Schwellenwert `background_consolidation_episode_threshold` als automatischer Hintergrund-Trigger verdrahtet (`execute_consolidation_pass()` wird bei ausreichend Turns und keinen aktiven Agenten-Sessions aufgerufen).
- `consolidation_engine.rs` (bzw. entsprechendes Modul in `memfuse-db`) — **`ConsolidationEngine<S, V>`:** eigenständiger, per `tokio::spawn` gestarteter Background-Task, der den vollständigen Sleep-Cycle (Structural Consolidation Pass **und** Generative Synthesis Pass, `run_synthesis_pass()`) periodisch und automatisch ausführt, inklusive geordnetem Shutdown über ein Cancellation-Token. Damit ist das ursprüngliche Alleinstellungsmerkmal "Sleep Cycle" — automatische, LLM-basierte Gedächtniskonsolidierung ohne manuellen Trigger — vollständig realisiert, nicht mehr nur als aufrufbare Funktion vorhanden.
- `maintenance_config.rs` — Konfiguration für den Scheduler. Der `replicator_enabled`-Konfigurationswert ist korrekt auf `false` defaultet, konsistent mit der Entfernungsklassifikation von `memfuse-calibration::replicator.rs`.
- `background_workers.rs` — generische Hintergrund-Task-Infrastruktur.
- `transaction.rs` — `DbTransaction`.
- `volatile_vault.rs` — verschlüsselter volatiler Speicher (Grundlage für MCP-Sandbox, nutzt `memfuse-crypto::kv_segment`).
- `export.rs`/analoges Modul — **Memory-Export-Format v1**: erzeugt ein portables, versioniertes JSON-Exportformat je Collection, das Dokumente, Roh-Embeddings, das verwendete `embedding_model`, `importance_score` je Eintrag sowie alle bekannten `relations` (Graph-Kanten) verlustarm serialisiert. Dies ist die technische Grundlage für Backup, Migration zwischen Instanzen und Interoperabilität mit externen Tools.

**Konfigurationstyp `EmbeddingBackend` (Enum in `MemFuseConfig`):** Varianten `Onnx { model_name, cache_dir }` (**Default**), `Ollama { base_url, model }`, `Candle { model_dir, quantization }`, `None`. Diese Enum-Einführung ersetzt die vormals implizite, ausschließlich Ollama-basierte Backend-Auswahl vollständig.

**Feature-Flags:** `default = []`, `bench`, `sandbox`, `experimental-diskann` (durchgereicht), `reranking` (aktiviert `memfuse-embed/onnx`), `background-maintenance`, `graph-connectivity-health` (durchgereicht), `replicator-dynamics-weights` (durchgereicht, zur Entfernung markiert), `coherence-bonus-fusion`, `adaptive-candidate-pool-sizing`, `volatile-vault`, `edge-reinforcement-learning` (durchgereicht).

**Bekannte Code-Hygiene-Anmerkung (weiterhin offen):** `Cargo.toml` führt `memfuse-candle` zusätzlich als ungenutzte `[dev-dependencies]`-Eintragung, obwohl das Crate produktiv bereits über `[dependencies]` eingebunden ist — diese doppelte, in `tests/`/`benches/`/`examples/` nirgends referenzierte Dev-Abhängigkeit ist vor dem Release zu entfernen.

**Status:** ✅ Kern. Sowohl der deterministische Structural Pass als auch der LLM-basierte Generative Synthesis Pass laufen automatisch über die `ConsolidationEngine`; das "Sleep-Cycle"-Alleinstellungsmerkmal ist vollständig realisiert.

### §4.14 `memfuse-router` — Layer 6

Conformal Router mit outcome-kalibriertem Routing und proaktiver Drift-Überwachung. Abhängigkeiten: `memfuse-core`, `memfuse-store`, `memfuse-db`, `memfuse-ollama`.

- `router.rs` — `RouterEngine`: zentrale Routing-Logik. Kernmethoden: `route()` (async), `profiles()`, `update_profiles()`/`try_update_profiles()`, `calibration_stats()` (liefert `HashMap<String, ProfileCalibrationState>`), `reset_calibration()`, `drift_status()` (liefert `Option<LyapunovResult>` pro Profil), `set_lyapunov_baseline()`, `record_outcome()`, `pending_decision_count()`, `reset_all_calibration()`. Zustandstypen: `ConfidenceMetrics`, `RoutingDecision`, `RouterState`.
- `lyapunov.rs` — Lyapunov-Drift-Watcher: KL-Divergenz über 10-Bin-Histogramm mit Laplace-1-Glättung, diskreter Lyapunov-Exponent über gleitendes Fenster, mit Konfidenzintervallen.
- `dispatch.rs` — Dispatch-Logik für SLM-Profile.
- `outcome.rs` — `RoutingOutcome`-Typen.
- `profile.rs` — `SlmProfile`-Definitionen.
- `serde_helpers.rs` — Serialisierungs-Hilfsfunktionen.
- `tests.rs` — umfangreiches, eigenständiges Testmodul (macht den Großteil der Crate-Testabdeckung aus) und ist als reguläres Modul der Crate zu behandeln, nicht als Nebenprodukt.

**Architekturschwäche (gelöst):** `RouterEngine` hält Profile, Kalibrierung und Drift-Watcher inzwischen gebündelt in einem einzigen `ArcSwap<RouterState>`, wodurch Hot-Reloads des Calibration-State atomar sind (Lese-/Schreibpfade sehen den Zustand stets konsistent als Einheit). Ein separates `RwLock<HashMap<DecisionId, (String, Instant)>>` existiert weiterhin ausschließlich für `pending_decisions` — dies ist ein bewusst getrennter, unkritischer Nebenzustand (hochfrequente Schreibzugriffe für Outcome-Tracking) und keine Konsistenzlücke.

**Grenzschicht-Exposition (gelöst):** Die Drift-/Kalibrierungsdaten (`drift_status()`, `calibration_stats()`) sind als öffentliche Rust-API abfragbar **und** bis in die Python-Grenzschicht exponiert: `PyDbStats` führt inzwischen die Felder `drift_status` ("stabil"/"warnung"/"kritisch"/"unbekannt"), `calibration_ece` (Expected Calibration Error) sowie `last_calibration_at` (UNIX-Timestamp des letzten Kalibrierungs-Rebuilds). **Status:** ✅ Kern (Routing/Drift-Logik), ✅ Grenzschicht-Exposition realisiert.

### §4.15 `memfuse-agent` — Layer 7

Persistenter Agenten-Workflow-Loop nach dem Muster `checkpoint → execute → commit → audit`. Selbstbeschrieben als "souveräne Alternative zu LangGraph/AutoGen: pure Rust, keine externen Abhängigkeiten". Abhängigkeiten: `memfuse-core`, `memfuse-db`, `memfuse-graph`, `memfuse-checkpoint`, `memfuse-store`, `memfuse-router`, `memfuse-index`, `memfuse-text`.

- `engine.rs` — `OrchestratorEngine`: `new()`, `from_db()`, `try_register_tool()`, `recover_orphans()` (async), `run()` (async, nimmt `AgentContext` + `StateGraph`), `replay_from()` (async), `checkpoint()` (async), `run_event_loop()` (async). `EventLoopExitReason`-Enum.
- `graph.rs` — `StateGraph`: deklarativer Workflow-Graph mit `AgentNode`/`NodeType`, `WorkflowEdge`. Methoden `try_add_node()`/`add_node()`, `try_add_edge()`/`add_edge()` (mit Bedingung und Priorität), `get_node()`.
- `step.rs` — einzelne Ausführungsschritte.
- `context.rs` — `AgentContext`: Laufzeitzustand während der Ausführung.
- `dlq.rs` — Dead-Letter-Queue für fehlgeschlagene Schritte.
- `audit.rs` — unveränderliches Audit-Logging über LSM-persistierte Keys.
- `event_source.rs` — Event-Sourcing-Infrastruktur.

**Zentrale Invarianten:** Token-Budget-Enforcement über `memfuse-core::types::budget`; State-Machine-Diagramm im Datei-Header von `lib.rs` dokumentiert. **Status:** ✅ Kern.

### §4.16 `memfuse-mcp` — Layer 8 (primäre Grenzschicht)

Model-Context-Protocol-Server — stdio-basiertes JSON-RPC-2.0-Interface, primärer Vertriebsweg. Im Code selbst als "Layer-7-Rand-Crate ohne jegliche `unsafe`-Toleranz — verarbeitet direkt untrusted stdio-Input" beschrieben. Abhängigkeiten: `memfuse-db`, `memfuse-core`, `memfuse-security`, `memfuse-ollama`, `memfuse-embed` (optional), `memfuse-agent` (optional), `memfuse-candle` (optional).

- `bin/memfuse-mcp-server.rs` — Binary-Entry-Point. Loggt ausschließlich nach `stderr` (`stdout` ist strikt für JSON-RPC-Transport reserviert). CLI-Flags: `--db-path`, `--provider`, `--ollama-url`, `--embed-model`, `--onnx-model-path`, `--read-only`, `--allow-write`.
- `lib.rs` — `McpServer`: zentrale Server-Struktur. Konstruktoren `new()`, `with_write_permission()`, `with_sandbox()`, `with_injection_guard()`. Implementiert die JSON-RPC-Methoden `initialize`, `initialized`, `tools/list`, `tools/call`, `ping`.
  - **Vier MCP-Tools:**
    1. `memfuse_search` — Hybrid Semantic Search (Vektor + BM25 + Graph); Eingabe `query` (required), `collection` (default `"default"`), `k` (default `10`); Beschreibung enthält explizit einen Sicherheitshinweis, dass zurückgegebener Inhalt aus untrusted Dokumenten stammt und im Client in `<untrusted_context>`-Tags isoliert werden muss.
    2. `memfuse_insert` — Dokument einspeichern mit Auto-Embedding und Auto-Chunking (`MarkdownChunker`, ~512 Tokens); Eingabe `id`, `text` (required), `collection`, `metadata`.
    3. `memfuse_get` — Dokument per ID abrufen; gleicher Untrusted-Content-Sicherheitshinweis.
    4. `memfuse_collections` — alle Collections auflisten.
  - **Weiterhin offen:** Ein fünftes Tool `memfuse_consolidate` als manueller Trigger für `execute_consolidation_pass`/`run_synthesis_pass` ist Teil des Produktumfangs und noch nicht implementiert. Da die Konsolidierung inzwischen automatisch über die `ConsolidationEngine` (§4.13) läuft, sinkt die Priorität dieses Tools von "Kernfunktion" auf "manuelles Debugging-/Admin-Werkzeug", bleibt aber verbindlicher Teil des MCP-Vertriebswegs für Nutzer, die einen sofortigen Konsolidierungslauf erzwingen wollen.
- `config.rs` — `EmbeddingConfig` (inkl. `from_env()`, `build_provider()`), `is_write_allowed_by_env()`.
- `prompt_injection.rs` — Prompt Injection Guard: NFKC-Unicode-Normalisierung vor Pattern-Matching, Zero-Width-Character-Entfernung, rekursive Base64-Payload-Dekodierung (bis Tiefe 2). Quarantäne-Modi: Strict (redact), Escalate (log + redact), Passthrough.
- `sandbox.rs` — MCP-Sandbox: `execute_with_timeout()` umschließt jeden Tool-Aufruf; volatile Tool-Outputs werden AES-256-GCM-SIV-verschlüsselt im RAM gehalten (Zeroize-on-Drop); Whitelist-Policy (Read/Write/CodeExecution als separate Permissions), Default: nur Lesen erlaubt.
- `protocol.rs` — `McpError`-Enum mit JSON-RPC-Standardfehlercodes, `response_from_error()`.

**Feature-Flags:** `default = []`, `agent-workflows` (aktiviert `memfuse-agent`), `onnx` (aktiviert `memfuse-embed`/`memfuse-embed::onnx`), `candle` (optionale Abhängigkeit zu `memfuse-candle`), `test-utils`.

**Packaging:** Ein `pyproject.toml` existiert im Crate-Verzeichnis — die `uvx`-Installierbarkeit (`uvx memfuse-mcp --db-path ...`) ist damit als Distributionsmechanismus vorhanden. Server-Version im `initialize`-Response ist weiterhin `"0.1.0"`. **Status:** ✅ Kern-Protokoll implementiert und `uvx`-paketiert, ⏳ `memfuse_consolidate`-Tool offen (Kalibrierungs-/Drift-Exposition ist bereits über `memfuse-py` gelöst, siehe §4.14).

### §4.17 `memfuse-py` — Grenzschicht (isoliertes FFI-Workspace)

Python-Bindings via PyO3, sekundärer Vertriebsweg. Läuft in einem eigenen, vom Hauptworkspace isolierten Cargo-Workspace mit `panic = "unwind"`-Profil (bewusst nicht in `workspace.members` der Root-`Cargo.toml` gelistet). Abhängigkeiten: `memfuse-core`, `memfuse-db`.

**Öffentliche Python-API (PyO3-Modul `_memfuse`):**
- **Modul-Einstiegspunkt:** `open(path, dimension=1536, max_elements=None, encryption_passphrase=None, distance_metric=None)` → `PyMemFuse`. Validiert `path`, `dimension` (1–10.000), `max_elements` (>0), `distance_metric` ∈ {`cosine`, `euclidean`/`l2`, `dot`/`dotproduct`}.
- **`PyMemFuse`** (Haupt-Facade): `worker_threads()`, `collection(name)` → `PyCollection`, `list_collections()`, `drop_collection(name)`, `flush()`, `stats()` → `PyDbStats`, `len()`, `is_empty()`.
- **`PyCollection`**: `stats()` → `PyVectorIndexStats`, `len()`, `is_empty()`, sowie alle CRUD-/Such-Methoden aus dem `memfuse_crud_methods!`-Makro.
- **`memfuse_crud_methods!`-Makro**, generiert identisch für `PyMemFuse` und `PyCollection` (eliminiert ca. 400 Zeilen Duplikation): `insert()`, `get()`, `update()`, `upsert()`, `delete()`, `search()`, `search_fb()` (FlatBuffer-Antwort), `hybrid_search()`, `hybrid_search_fb()`, `relate()`, `scan_prefix()`, `scan()`, sowie Batch-Varianten `insert_many()`, `upsert_many()`.
- **Statistik-Typen:** `PyDbStats { index_stats: PyVectorIndexStats, storage_stats: PyStorageStats }`, `PyVectorIndexStats`, `PyStorageStats` (`num_segments`, `total_size_bytes`, `memtable_size_bytes`).
- **`PySearchResult`**, **`PyDocument`** — Rückgabetypen.
- Testhilfsfunktion `_trigger_panic_for_test()` zur Verifikation der FFI-Panic-Isolation.

**Zero-Copy-Strategie:** Eingabe-Vektordaten werden aus NumPy-Arrays zero-copy nach Rust geborgt; FlatBuffer-Antworten werden als `PyBytes` zurückgegeben.

**Runtime-Modell:** gemeinsame Tokio-Runtime pro Python-Interpreter (`MEMFUSE_WORKER_THREADS`-Env-Var konfigurierbar, Default: `verfügbare_parallelität / 2`, minimal 2 Worker); Subinterpreter-Guard (`check_subinterpreter_guard`) verhindert Mehrfachinitialisierung in inkompatiblen Python-Subinterpreter-Kontexten.

**Erweiterte Statistik-API (neu):** `PyDbStats` wurde um `drift_status: String`, `calibration_ece: Option<f32>` und `last_calibration_at: Option<u64>` ergänzt (siehe §4.14) — die vormalige Beobachtungslücke zwischen `memfuse-router` und der Python-Grenzschicht ist geschlossen.

**Offene, vor Release verbindlich zu behebende Inkonsistenzen:**
1. **Dimension-Default:** `open()` verwendet weiterhin `dimension=1536`, während `MemFuseConfig::default().dimension` in `memfuse-db` `768` (nomic-embed-text-Fallback) ist. Beide Werte müssen vereinheitlicht werden, empfohlen auf `768` als konsistenten ONNX-Default.
2. **Versionsnummer:** Modul-Attribut `__version__` ist weiterhin `"0.2.0"`, während `Cargo.toml` weiterhin `version = "0.1.0"` deklariert.

**Status:** ✅ Kern-API implementiert und um Observability-Felder erweitert, ⏳ zwei dokumentierte Namens-/Versionsinkonsistenzen offen.

---

## §5 — Systemweite Datenflüsse

### §5.1 Schreibpfad (`insert`/`upsert`)

`memfuse-mcp::memfuse_insert` oder `memfuse-py::insert()` → `memfuse-db::collection::crud` → Chunking (`chunker.rs`, falls Markdown) → Embedding-Erzeugung (Default: `memfuse-embed`/ONNX; alternativ `memfuse-ollama` oder `memfuse-candle`, konfigurierbar über `EmbeddingBackend`) → `memfuse-core::TxBuffer`-Staging → `memfuse-store::wal.rs` (WAL-First, ggf. `memfuse-crypto::wal_crypto` verschlüsselt) → `memfuse-store::memtable.rs` → asynchron `memfuse-index::hnsw.rs` (Vektor) + `memfuse-text::bm25.rs`/`inverted.rs` (Volltext) + `memfuse-graph::csr.rs` (falls `relate()` genutzt wurde).

### §5.2 Lesepfad (`search`/`hybrid_search`)

Anfrage → parallele Ausführung der vier Signale (HNSW-kNN, BM25-Score, CSR-Graph-Traversal/PathRAG, Metadaten-Filter) → `memfuse-db::fusion.rs` (RRF, optionaler Kohärenz-Bonus) → optionales Cross-Encoder-Reranking (`memfuse-embed::reranker.rs`, hinter `reranking`-Feature) → optionale Isotonic-Kalibrierung der finalen Scores (`memfuse-calibration`) → Rückgabe an Aufrufer. Bei Nutzung über `memfuse-router` zusätzlich: Konfidenzbewertung, Drift-Check (`lyapunov.rs`) und ggf. Routing-Entscheidung zwischen SLM-Profilen.

### §5.3 Konsolidierungspfad ("Sleep Cycle")

Zwei sich ergänzende Automatismen realisieren den vollständigen Sleep-Cycle: (a) `MaintenanceScheduler` prüft periodisch `background_consolidation_episode_threshold` gegen die Anzahl neuer Turns und löst bei Überschreitung sowie Abwesenheit aktiver Agenten-Sessions `execute_consolidation_pass()` aus; (b) die eigenständige, per `tokio::spawn` laufende `ConsolidationEngine` führt periodisch den vollständigen Zyklus aus Structural Consolidation (`memory_consolidation.rs`, deterministisch, Sliding-Window-Clustering, Near-Duplicate-Erkennung, Waisenkanten-Identifikation) **und** Generative Synthesis Pass (`synthesis_phase.rs::run_synthesis_pass()`, LLM-basiert, erzeugt abstrakte `SynthesizedChunk`s) automatisch aus, bis sie über ein Cancellation-Token geordnet beendet wird. In beiden Pfaden wendet `consolidation_executor.rs` anschließend Tombstones und — bei Supersedes-Ereignissen — automatische Graph-Cascade-Invalidierung (`memfuse_graph::cascade_invalidate_edges_for_superseded_doc`) an; derselbe Cascade-Aufruf erfolgt zusätzlich synchron im regulären CRUD-Schreibpfad (`collection/crud.rs`), sodass Supersedes-Ereignisse unabhängig vom Konsolidierungszeitpunkt sofort auf den Graphen wirken.

### §5.4 Exportpfad

`memfuse-db`-Exportfunktion → serialisiert eine Collection vollständig (Dokumente, Embeddings, `embedding_model`, `importance_score`, `relations`) als versioniertes JSON (`memfuse-export-v1.json`) → über `memfuse-py` oder direkt auf Rust-Ebene abrufbar → Grundlage für Backup, Instanzmigration und Interoperabilität mit Fremdwerkzeugen.

---

## §6 — Feature-Klassifikation (verbindlich)

**Kernfeatures (dauerhaft, produktdefinierend):** 4-Signal-RRF-Fusion inkl. PathRAG (F-09/Graph), Structural Consolidation Pass, Generative Synthesis Pass, DecayController (F-01), Immunologische Widerspruchsabwehr (F-04), Session-DAG mit `NodesGuard`, WAL-HMAC-Kette, `DeletionProof`, MCP-Sandbox mit Prompt Injection Guard, Memory-Export-Format v1.

**Optionale Features (hinter Feature-Flag, dauerhaft unterstützt):** Graph-Connectivity-Health/Perkolation (F-06), Edge-Reinforcement-Learning/hebbianisches Kanten-Reinforcement (F-03), Coherence-Bonus-Fusion (F-09-Erweiterung), Adaptive-Candidate-Pool-Sizing, PID-Homöostase (F-08), Cross-Encoder-Reranking.

**Zur Entfernung markiert (nicht Teil des Endprodukts):** Replicator-Dynamics-Gewichtung (F-07, `memfuse-calibration::replicator.rs`) — physisch aus dem Code zu entfernen, sobald der zugehörige Konfigurationspfad vollständig entkoppelt ist.

**Permanent verworfen (kein Zukunftsvorhaben):** Desktop-App (`memfuse-tauri`, physisch entfernt), Voice-Assistant-Interface, Cross-Tenant-Wissensaustausch (bricht `TenantId`-Isolation und `DeletionProof`), dateisystembasiertes Claim-Locking (durch atomare GitHub-Issues/Labels-API ersetzt), verteilte ADR-Dateien (durch zentrale `DECISIONS.md` ersetzt).

**Dauerhaft gesperrt bis Architektur-Revision (VETO):** Partial-HNSW-Rebuild (`memfuse-index::partial_rebuild.rs`, F-02) — verletzt Delaunay-Nachbarschaften, führt zu Recall-Kollaps; Ersatz ist der 2-Phasen-CoW-Rebuild.

---

## §7 — Sicherheits- und Datenschutzmodell

1. **Zero-Trust gegenüber Tool-Output:** Jeder von `memfuse_search`/`memfuse_get` zurückgegebene Inhalt gilt als untrusted und muss vom aufrufenden Client isoliert (z. B. in `<untrusted_context>`-Tags) behandelt werden.
2. **Prompt Injection Guard:** NFKC-Normalisierung, Zero-Width-Character-Entfernung, rekursive Base64-Dekodierung bis Tiefe 2, mit drei konfigurierbaren Reaktionsmodi (Strict/Escalate/Passthrough).
3. **Volatile-Vault-Verschlüsselung:** Tool-Outputs werden im MCP-Sandbox-Kontext AES-256-GCM-SIV-verschlüsselt im RAM gehalten und bei Drop gezeroized.
4. **Integritätskette:** WAL-Einträge sind per HMAC-Kette gegen nachträgliche Manipulation gesichert.
5. **Kryptographischer Löschnachweis:** `DeletionProof` erbringt einen negativen Rekonstruktionsnachweis für gelöschte Daten (DSGVO Art. 17), mit expliziter `ExcludedScope`-Deklaration für Fälle, in denen gelöschte Inhalte dennoch implizit in LLM-Fine-Tuning-Parametern nachwirken könnten — dieser Ausschluss ist gegenüber Nutzern transparent zu dokumentieren.
6. **Whitelist-Berechtigungsmodell:** Read/Write/CodeExecution als getrennte, im MCP-Server konfigurierbare Permissions; Default ist ausschließlich Lesezugriff.
7. **Kein Multi-Tenant-Fremdkundenbetrieb:** `TenantId` dient ausschließlich Prozess-/Test-Isolation, nicht der gleichzeitigen Bedienung fremder Endkunden auf gemeinsamer Infrastruktur — dies ist eine bewusste Grenze, kein technisches Defizit.

---

## §8 — Bekannte Architektur- und Härtungsthemen (Stand des Endprodukts)

Diese Liste beschreibt den tatsächlichen, aktuell verbleibenden Härtungsbedarf. Bereits behobene historische Probleme sind **nicht** mehr Teil dieser Liste, da sie im Code nachweislich bereits gelöst sind — dazu zählen inzwischen: Group-Commit-Rollback-Propagation, fehlende Crash-Recovery für Pending-Intents, fehlendes `--release`-Flag/TTL-Expiry im Claim-System, unvollständiger Cascade-Trigger Supersedes→Graph-Kanten-Tombstone (H-3, jetzt sowohl im CRUD-Pfad als auch im Consolidation-Executor verdrahtet), fehlende Kalibrierungs-/Drift-Exposition in `PyDbStats` (H-11, jetzt vorhanden), fehlender automatischer Trigger für den Generative Synthesis Pass (H-12, jetzt via `ConsolidationEngine` gelöst), sowie das Fehlen eines `LlmTextGeneratorStreaming`-Traits (jetzt implementiert).

| ID | Bereich | Beschreibung | Einstufung |
|:---|:---|:---|:---:|
| H-1 | `memfuse-store` | WAL-I/O teilweise unter Write-Lock; Latenzrisiko unter hoher Last | Beobachten |
| H-2 | `memfuse-store` | CheckpointPin-TOCTOU-Fenster unter aggressiver Compaction | Beobachten |
| H-5 | `memfuse-candle::inference.rs` | Kein Backpressure-Vertrag (Semaphore) analog `memfuse-embed`; Inferenzaufrufe potenziell blockierend | Vor breitem Candle-Rollout schließen |
| H-6 | `memfuse-db` (Symbolnamen) | `memory_consolidation.rs::run_structural_synthesis_pass()` und `synthesis_phase.rs::run_synthesis_pass()` tragen sehr ähnliche Namen für unterschiedliche Funktionen (deterministischer Struktur-Pass vs. LLM-basierter Synthesis-Pass) — Verwechslungsgefahr trotz bereits unterschiedlicher Benennung | Doku-Kommentar/Modul-Header schärfen, keine Umbenennung mehr zwingend nötig |
| H-7 | `memfuse-db/Cargo.toml` | Ungenutzte `dev-dependency` auf `memfuse-candle` (Crate ist bereits produktiv über `[dependencies]` eingebunden) | Aufräumen |
| H-8 | `memfuse-py` | Dimension-Default-Inkonsistenz (`open()`: 1536 vs. `MemFuseConfig::default()`: 768) | Vor Release vereinheitlichen |
| H-9 | `memfuse-py` | Versionsnummer-Inkonsistenz (`__version__` "0.2.0" vs. `Cargo.toml` "0.1.0") | Vor Release vereinheitlichen |
| H-10 | `memfuse-crypto`/`memfuse-security` | Verzeichnis- vs. Package-Name uneinheitlich in Doku-Referenzen | Als Synonym dokumentieren oder vereinheitlichen |
| H-13 | `memfuse-mcp` | Kein `memfuse_consolidate`-Tool (manueller Admin-Trigger für einen sofortigen Konsolidierungslauf) | Fünftes MCP-Tool ergänzen |
| H-14 | Test-Infrastruktur | Aktuelle Testfunktionszahl sollte gegen `cargo nextest list` gegenverifiziert werden (grep-basierte Zählung ist approximativ) | Verifizieren |
| H-15 | `AGENTS.md` (Root + Crate-lokal) | Verifizierter-Codestand-Anker (`HEAD`, Datum) in mehreren `AGENTS.md`-Dateien sowie in `docs/GESAMTSPEZIFIKATION_v10.md` liegt hinter dem tatsächlichen `HEAD`; P17/P19 verlangen Aktualität dieser Dateien | Nach jedem Merge aktualisieren (`cargo xtask sync-docs`, falls vorhanden) |
| H-16 | Code-Kommentare (`memfuse-db::maintenance_scheduler.rs` u. a.) | Verweise auf §-Nummerierungen eines internen, nicht mehr aktuellen Architekturdokuments statt auf `DECISIONS.md`/diese Spezifikation | Referenzen auf ADR-Nummern vereinheitlichen |

**Silent-Data-Corruption-Risiken:** Keine derzeit offen. Verbleibende Punkte (H-1, H-2, H-4 bis H-10, H-13 bis H-16) sind Latenz-, Konsistenz-Fenster-, Namens-, Dokumentations- oder Vollständigkeitsthemen, keine Korruptionsrisiken.

---

## §9 — Betriebsmodi

| Modus | Air-Gap-fähig | Externe Prozessabhängigkeit | Vertriebsreife |
|---|:---:|:---:|:---:|
| ONNX-Embedding (Default) | ✅ | ❌ | ✅ Release-Default |
| Candle-Embedding/-Inferenz | ✅ | ❌ | ⏳ Backpressure-Härtung offen (H-5) |
| Ollama-Embedding/-Inferenz | ❌ | ✅ (Ollama-Prozess) | ✅ voll unterstützt, nicht mehr Default |
| MCP stdio + Zero-Trust-Sandbox | ✅ | ❌ | ✅ |
| Öffentliches Release (PyPI, crates.io, `uvx`) | — | — | ⏳ siehe §11 |

**Benchmark-Strategie:** `bench.yml` läuft aktuell gegen zwei Fixture-Testfälle (LongMemEval- und LoCoMo-Kurzform), ohne automatisierten Dataset-Download/-Cache für die vollständigen Benchmarks. Dies läuft parallel zur Release-Vorbereitung, ist aber kein Release-Blocker. Aktiver Vergleichsrahmen: Mem0, Zep/Graphiti, VelesDB.

---

## §10 — Governance & Entwicklungsprozess

### §10.1 Fünf Säulen des Entwicklungssystems

1. **Preflight Gate (`xtask/src/jules_preflight.rs`):** zentraler Aggregator aller lokalen und CI-Gates.
2. **Anti-Collision Claim System (`xtask/src/claim.rs`):** `cargo xtask claim --crate X --issue Y` sperrt Ziel-Crates via GitHub-Issues/Labels; `--release`-Flag und automatische TTL-Expiry (4h) sind implementiert und aktiv.
3. **Single Source of Truth:** `DECISIONS.md` als einzige ADR-Quelle; `WORKING_STATE.md` autogeneriert. Code-Kommentare, die auf ein internes, nicht mehr aktuelles Architekturdokument mit abweichender §-Nummerierung verweisen, sind zu bereinigen, um Verwechslung mit dieser Spezifikation auszuschließen.
4. **Gehärtete CI-Guardrails:** 14 Workflows (`bench.yml`, `build-wheels.yml`, `chaos.yml`, `context-gates.yml`, `merge-gate.yml`, `mutation-testing.yml`, `nucleation-recall-history.yml`, `post-merge-verification.yml`, `prune-branches.yml`, `publish-pypi.yml`, `rust-ci.yml`, `scheduled-audit.yml`, `update-prompter-data.yml`, sowie ein neuer/verbleibender Workflow als Ersatz für das entfernte `tauri-release.yml`).
5. **Prompter & Bootstrap Protocol:** unüberspringbarer Mandatory-Bootstrap-Präfix in jeder Session (`AGENTS.md`, `.jules/SESSION_BOOTSTRAP.md`).

### §10.2 Zwei-Stufen-Entwicklungsprozess

- **Stufe 1 (Orchestrator):** liest Repository-Zustand, prüft Architektur/ADRs, trifft Entscheidungen, verfasst präzise Task-Spezifikationen — schreibt keinen Produktionscode.
- **Stufe 2 (Ausführender Agent):** führt Mandatory Bootstrap aus, setzt Crate-Claim, führt Preflight-Gate aus, setzt exakt die spezifizierten Änderungen um. Keine eigenständigen ADRs; bei Bedarf `ADR-VORSCHLAG:` im PR-Body.

### §10.3 Sprache

Deutsch bleibt Sprache der internen Governance-Dokumentation; Code-Kommentare und öffentliche API-Dokumentation (README, PyPI, MCP-Server-Doku) werden konsequent auf Englisch geführt.

### §10.4 Lizenz & Contributions

MIT OR Apache-2.0. Komplett kostenlos & Open Source, keine Monetarisierungsabsicht im Kern. Externe menschliche Contributions werden vorerst nicht aktiv beworben.

---

## §11 — Priorisierte Roadmap zum Endprodukt

Der folgende Fahrplan spiegelt den tatsächlichen, verifizierten Fortschritt wider: Ein erheblicher Teil der ursprünglichen Woche-1-Aufgaben ist bereits umgesetzt.

### Bereits umgesetzt (verifiziert im aktuellen Code, HEAD `dabdc63`)

- `memfuse-tauri` vollständig entfernt (inkl. Workspace-Mitgliedschaft).
- ONNX als Default-`EmbeddingBackend` verdrahtet; `EmbeddingBackend`-Enum in `MemFuseConfig` eingeführt (Varianten Onnx/Ollama/Candle/None).
- `memfuse-mcp` als `uvx`-kompatibles Python-Paket verfügbar (`pyproject.toml` vorhanden).
- `memfuse-candle` als echter, konfigurierbarer Embedding-Backend-Pfad verdrahtet.
- Memory-Export-Format v1 implementiert (Dokumente, Embeddings, `embedding_model`, `importance_score`, `relations`).
- `replicator_enabled`-Default korrekt auf `false` gesetzt.
- Claim-System um `--release`-Flag und TTL-Expiry (4h) ergänzt.
- Cascade-Trigger Supersedes→Graph-Kanten-Tombstone vollständig verdrahtet (CRUD-Pfad **und** Consolidation-Executor).
- `LlmTextGeneratorStreaming`-Trait implementiert und in `memfuse-candle` sowie `memfuse-ollama` umgesetzt.
- `ConsolidationEngine`-Hintergrund-Task implementiert: Generative Synthesis Pass läuft jetzt automatisch, kein rein manueller Trigger mehr nötig.
- Kalibrierungs-/Drift-Metriken (`drift_status`, `calibration_ece`, `last_calibration_at`) in `PyDbStats` exponiert.

### Verbleibend bis zum ersten Alpha-Release (P0)

| Aufgabe | Crate(s) |
|---|---|
| Dimension-Default vereinheitlichen (1536 vs. 768) | `memfuse-py`, `memfuse-db` |
| Versionsnummer vereinheitlichen (`__version__` vs. `Cargo.toml`) | `memfuse-py` |
| `memfuse-py`-Wheels für Linux/macOS/Windows real bauen und verifizieren (Workflow existiert bereits) | `memfuse-py` |
| Ungenutzte `dev-dependency` auf `memfuse-candle` aus `memfuse-db` entfernen | `memfuse-db` |
| Namenskonsistenz `memfuse-crypto`/`memfuse-security` dokumentieren | `memfuse-crypto` |
| README auf 5-Minuten-Quickstart fokussieren | Root |
| Backpressure-Vertrag für `memfuse-candle::inference.rs` ergänzen | `memfuse-candle` |
| `AGENTS.md`/`docs/GESAMTSPEZIFIKATION_v10.md`-Anker auf aktuellen `HEAD` nachziehen | Root, alle Crates |

**Ergebnis:** Erstes veröffentlichbares `v0.1.0-alpha`.

### Woche 2 — MCP-Server als Hauptprodukt vervollständigen

| Aufgabe | Crate(s) |
|---|---|
| Claude-Desktop-Config-Snippet in Doku | `memfuse-mcp` |
| MCP-Tool `memfuse_consolidate` (manueller Admin-Trigger für einen sofortigen Konsolidierungslauf, ergänzend zur automatischen `ConsolidationEngine`) | `memfuse-mcp`, `memfuse-db` |

### Woche 3–4 — Restliche Härtung

| Aufgabe | Crate(s) |
|---|---|
| `run_synthesis_pass`/Struktur-Helfer-Namenskollision auflösen | `memfuse-db` |
| Drei `RwLock`s in `RouterEngine` zu einem atomaren Zustand konsolidieren | `memfuse-router` |
| Code-Kommentare mit veralteten §-Nummerierungen auf `DECISIONS.md`-ADR-Verweise umstellen | `memfuse-db`, ggf. weitere |

### Woche 5–6 — Qualitätsnachweis

| Aufgabe | Crate(s) |
|---|---|
| LongMemEval/LoCoMo-Volldatensätze mit Cache-Mechanismus anbinden, Vergleich gegen Mem0, Zep, VelesDB | `memfuse-bench` |
| `pip install memfuse` PyPI-`v0.1.0`-Release offiziell taggen | `memfuse-py`, `publish-pypi.yml` |

### Nach dem Release — Crate-Konsolidierung (nicht vor GA)

```
KERN (6 Module, Fusionsziel):
  memfuse-core         [unverändert]
  memfuse-security     [Fusion: memfuse-crypto]
  memfuse-persistence  [Fusion: memfuse-store + memfuse-checkpoint]
  memfuse-retrieval    [Fusion: memfuse-index + memfuse-graph + memfuse-text]
  memfuse-orchestrator [memfuse-db, Scheduler-Konsolidierung]
  memfuse-inference    [Fusion: memfuse-calibration + memfuse-ollama + memfuse-candle + memfuse-router + memfuse-embed]

GRENZSCHICHT:
  memfuse-mcp          [unverändert]
  memfuse-py           [unverändert]
  memfuse-agentic      [memfuse-agent]

WERKZEUG:
  xtask, memfuse-bench [unverändert]
```

Migrationsreihenfolge: Phase 1a (Security) → Phase 1b (Persistence) → Phase 2 (Inference) → Phase 3 (Scheduler) → Phase 4 (Retrieval) → Phase 5 (Bereinigung, finale `cargo xtask check-dag`-Prüfung). Beginnt erst nach Abschluss des Alpha-Release.

---

## §12 — Definition of Done

Ein Milestone oder Release gilt als "Done", wenn:

1. `cargo test --workspace` besteht fehlerfrei mit 0 Regressionen.
2. `uvx memfuse-mcp` startet fehlerfrei gegen einen frischen `~/.memfuse`-Pfad und beantwortet eine `memfuse_search`-Anfrage ohne laufende Ollama-Instanz (ONNX-Default).
3. `pip install memfuse` installiert fehlerfrei auf Linux x86_64, macOS arm64 und Windows x86_64, und `memfuse.open()` → `mem.add()`/`insert()` → `search()` funktioniert ohne externe Abhängigkeiten.
4. Dimension- und Versionsnummer-Inkonsistenzen in `memfuse-py` sind behoben.
5. `DeletionProof` (`memfuse-crypto`) erbringt den negativen Rekonstruktionstest.
6. `cargo xtask check-dag` bestätigt 0 Layer-Verletzungen.
7. Ein Git-Tag `v0.1.0-alpha` existiert und `publish-pypi.yml` wurde erfolgreich ausgeführt.
8. Alle in §11 als "Verbleibend bis zum ersten Alpha-Release (P0)" gelisteten Punkte sind gelöst.
9. `AGENTS.md`/`WORKING_STATE.md` weichen maximal 3 Tage vom letzten Code-Commit ab.

---

## §13 — Glossar

- **RRF (Reciprocal Rank Fusion):** Fusion mehrerer Ranglisten unterschiedlicher Retrieval-Signale (`memfuse-db::fusion.rs`).
- **PathRAG:** Graph-basierte Retrieval-Methode mit bidirektionalem Dijkstra für Multi-Hop-Fragen (`memfuse-graph::path_rag.rs`).
- **ConfigFingerprint:** Hash-Fingerabdruck über Modell-/Kalibrierungsparameter zur automatischen Invalidierung veralteter Statistiken (`memfuse-core::types::domain`).
- **DeletionProof:** Kryptographischer Nachweis, dass gelöschte Daten auf Storage-Ebene nicht mehr rekonstruierbar sind (`memfuse-crypto::deletion_proof.rs`, DSGVO Art. 17).
- **Lyapunov-Drift-Watcher:** Statistisches Verfahren zur proaktiven Erkennung von Verteilungsverschiebungen in Kalibrierungs-Scores (`memfuse-router::lyapunov.rs`).
- **Session-DAG/`NodesGuard`:** Typsichere Datenstruktur zur Abbildung verzweigter Konversationen mit Compile-Time-Deadlock-Prävention (`memfuse-graph::session_dag.rs`).
- **Structural Consolidation Pass:** Deterministischer, LLM-freier Teil der Sleep-Cycle-Konsolidierung (`memfuse-db::memory_consolidation.rs`).
- **Generative Synthesis Pass:** LLM-basierter Teil der Sleep-Cycle-Konsolidierung, erzeugt abstrakte `SynthesizedChunk`s (`memfuse-db::synthesis_phase.rs`).
- **Memory-Export-Format v1:** Versioniertes, portables JSON-Exportformat je Collection inkl. Embeddings, Wichtigkeits-Scores und Beziehungen (`memfuse-db`).
- **EmbeddingBackend:** Konfigurations-Enum in `MemFuseConfig` mit Varianten Onnx (Default), Ollama, Candle, None — zentraler Auswahlmechanismus für den Inferenzpfad.

---

*Diese Spezifikation wurde erstellt am 2026-09-12 auf Basis einer eigenständigen Live-Verifikation des Quellcodes von `https://github.com/tfufuz1/memfuse` (HEAD `dabdc631`, > 150.000 Zeilen Rust, 17 Hauptworkspace-Crates + `memfuse-py`, 0 Releases). Sie berücksichtigt zwölf Commits, die seit dem ursprünglich referenzierten Stand (`87e80291`) eingeflossen sind — darunter die Entfernung von `memfuse-tauri`, die Umstellung auf ONNX als Default-Embedding-Backend, die `uvx`-Paketierung von `memfuse-mcp`, die Integration von `memfuse-candle` als Air-Gap-Pfad, das Memory-Export-Format v1, den `LlmTextGeneratorStreaming`-Trait, den automatisch laufenden `ConsolidationEngine`-Sleep-Cycle sowie die Kalibrierungs-/Drift-Exposition in `PyDbStats` — und dokumentiert diese als abgeschlossen, statt sie weiterhin als offen zu führen. Der verbleibende Weg zum Alpha-Release reduziert sich damit auf zwei Namens-/Versions-Inkonsistenzen in `memfuse-py`, ein fehlendes MCP-Admin-Tool und mehrere Code-/Dokumentations-Hygiene-Punkte (§8, §11).*
