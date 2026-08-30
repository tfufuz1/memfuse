# MemFuse Brain — GitHub Projekt- & Commit-Historie

> **Kanonische Dokumentation der Entwicklungshistorie von MemFuse Brain**
> *Zeitraum: August 2026 — Heute*

---

## 1. Übersicht & Meilenstein-Phasen

MemFuse Brain ist ein eingebettetes, air-gapped kognitives Betriebssystem und eine Desktop-Anwendung in Pure Rust. Die Projektgeschichte auf GitHub zeichnet die schrittweise Evolution von den mathematischen und speichertechnischen Kernschichten (Layer 0 & 1) über das Datenbank-Orchestrierungsmodul (Layer 2) und die LLM/FFI-Anbindungen (Layer 3) bis hin zur sicheren Desktop- und MCP-Server-Umgebung (Layer 4) nach.

### Phasenüberblick

| Phase | Zeitraum | Fokus & Kern-Errungenschaften |
|---|---|---|
| **Phase 1: DAG-Aktivierung & Multi-Crate Foundation** | 22.08.2026 – 24.08.2026 | DAG-Validierung, Entkopplung archivierter Crates, Fixes für DiskANN Bounds & Sector Size Checks, Ingestions-TxId Collision Fixes, HNSW Delete-Error Handling. |
| **Phase 2: RAG, Session-DAG & Cryptographic Hardening** | 25.08.2026 – 26.08.2026 | Anthropic Contextual Retrieval, Pure-Rust Session-DAG Branching, OsRng/AES-256-GCM-SIV & WAL HMAC Chaining, JSON-RPC 2.0 MCP-Server Basis. |
| **Phase 3: MVCC Durability, 2PC Transactions & Sync-Docs** | 27.08.2026 – 28.08.2026 | Full 4-Index 2-Phase Commit (2PC) in `memfuse-db`, WAL V3 Format mit `tx_id` HMAC Binding, Bi-temporale Graph-Gültigkeitsachsen, `xtask sync-docs` Werkzeuge. |
| **Phase 4: Robustness & Security Hardening Sprint** | 29.08.2026 – 30.08.2026 | Agent Event Loops, Memory Importance Decay, Zettelkasten Memory Links, Structured `MemFuseErrorDto`, Prompt Injection Guards in MCP, Zero-Copy LSM Scan, Write-Temp-Then-Rename für SSTables & DiskANN. |
| **Phase 5: Governance, Consistency & Quality Audit Pass** | 31.08.2026 – Heute | Erweiterung von `xtask check-consistency` (README, AGENTS.md, ADR Checks), CI Review Coverage Gates, `memfuse-index` Code Quality Refactoring (#1150). |

---

## 2. Chronologischer Commit-Verlauf

Hier sind die präzisen Commits der Entwicklungshistorie (chronologisch von den Anfängen bis heute):

### 22. August 2026
- `90633e3` | **google-labs-jules[bot]** | `Fix verify-dag CI workflow for memfuse-crypto and archived crates`
  *Anpassung der DAG-Prüfung in GitHub Actions zur korrekten Erkennung der Abhängigkeiten.*
- `65322dc` | **google-labs-jules[bot]** | `fix(ci): Allow memfuse-crypto dependency in memfuse-store DAG check`
  *Erlaubt die explizite Abhängigkeit von `memfuse-store` auf `memfuse-crypto` im DAG-Graph.*

### 23. August 2026
- `82c9966` | **google-labs-jules[bot]** | `Fix Ollama batch embedding stream lifetime and MCP test dimension configuration`
  *Behebung von Lifetime-Problemen bei gestreamten Ollama Batch-Embeddings und Angleichung der Dimensionen im MCP Test Harness.*

### 24. August 2026
- `f0c0335` | **google-labs-jules[bot]** | `fix(diskann): bounds check neighbor_count & resolve CI context-gates smells`
  *Sicherheitsüberprüfung für `neighbor_count` gegen `max_degree` in DiskANN zur Vermeidung von Out-of-Bounds Speicherallokationen.*
- `ae0cd14` | **google-labs-jules[bot]** | `Fix TxId collisions, add graph error logging, and resolve critical AI tags`
  *Behebung von Transaktions-ID-Kollisionen bei der Dokumenten-Ingestion und Hinzufügen von strukturierter Fehlerprotokollierung im Graph-Index.*
- `c4139b0` | **google-labs-jules[bot]** | `fix(diskann): validate sector_size on load() and resolve critical smell tags`
  *Validierung der `sector_size` beim Laden bestehender DiskANN-Indices zur Vermeidung lautloser Offset-Fehler.*

### 25. August 2026
- `548b885` | **google-labs-jules[bot]** | `feat: implement enforcement system for LLM-guided development`
  *Einführung automatisierter Governance-Gates für In-Code Anchor- und AI-Tag-Validierung.*
- `a83a165` | **google-labs-jules[bot]** | `ci: optimize context agent system and add enforcement gates`
  *Integration der CI-Schranken (`context-gates.yml`) zur Überprüfung von Sicherheits- und Qualitäts-Tags.*
- `38378aa` | **google-labs-jules[bot]** | `fix(memfuse-db): synchronize relate() API with CsrGraph index`
  *Synchronisierung der Beziehungs-API in `memfuse-db` mit dem zugrunde liegenden CSR-Graph-Index.*
- `2ba8782` | **google-labs-jules[bot]** | `fix(tauri): audit and verify escapeHtml usage for innerHTML XSS prevention`
  *Sicherheits-Audit der Tauri Frontend-UI bezüglich XSS-Prävention bei HTML-Sanitizing.*
- `79e71bb` | **google-labs-jules[bot]** | `fix(index): complete unsafe SAFETY proofs, DiskANN bounds, and HNSW rebuild test`
  *Vollständige Absicherung aller `unsafe`-Blöcke mit expliziten `SAFETY:` Nachweisen im `memfuse-index` Crate.*
- `d7172af` | **google-labs-jules[bot]** | `Audit and verify ONNX session pool and feature flags in memfuse-embed`
  *Sicherung der Thread-Sicherheit und Feature-Gating (`--features onnx`) für den ONNX Session Pool.*
- `6c14914` | **google-labs-jules[bot]** | `Audit memfuse-crypto cryptographic correctness and integrity`
  *Krypto-Audit für AES-256-GCM-SIV, HKDF-Schlüsselableitung und Zeroize-Garantien.*
- `f637e3a` | **google-labs-jules[bot]** | `Fix CI clippy and doc formatting issues`
  *Behebung von Linter-Warnungen und Formatierungsfehlern in der Dokumentation.*

### 26. August 2026
- `ef2d834` | **google-labs-jules[bot]** | `Harden cryptographic primitives in memfuse-crypto`
  *Härtung der kryptographischen Primitiven gegen Seitenkanal-Angriffe und Key-Reuse.*
- `28496bd` | **google-labs-jules[bot]** | `fix(crypto): enforce OsRng nonces, zeroization, and domain separation`
  *Erzwingung kryptographisch sicherer Zufalls-Nonces via `OsRng` und expliziter Domain Separation Tags.*
- `5f15fe6` | **google-labs-jules[bot]** | `Fix HNSW delete error swallowing and TTL reaper edge cases in memfuse-db`
  *Behebung des Verschluckens von Löschfehlern im HNSW Vektor-Index und Stabilisierung des Expiry Reapers.*
- `1cf2c55` | **google-labs-jules[bot]** | `fix(text): ensure tokenizer symmetry and BM25 parameter validation`
  *Validierung der BM25-Hyperparameter ($k_1, b$) und Sicherstellung symmetrischer Tokenisierung.*
- `ca75a52` | **google-labs-jules[bot]** | `fix(checkpoint): enforce pin/save/unpin ordering and safety`
  *Durchsetzung der RAII-Reihenfolge (`pin` -> `save` -> `unpin`) für atomare Speicher-Snapshots.*
- `c147c28` | **google-labs-jules[bot]** | `Harden Tauri command input validation and regex size limit`
  *Eingabewert-Validierung für Tauri-IPC-Befehle und Schutz vor ReDoS-Attacken durch Regex-Größenbeschränkung.*
- `e52d3ff` | **google-labs-jules[bot]** | `feat(mcp): enforce JSON-RPC 2.0 protocol compliance in memfuse-mcp`
  *Strikte Protokoll-Validierung für den MCP Stdio-Server gemäß JSON-RPC 2.0 Spezifikation.*
- `2de1caa` | **google-labs-jules[bot]** | `Fix MemTable shard selection using full-key BLAKE3 hash`
  *Präzise Verteilung von Einträgen auf MemTable-Shards mittels Vollschlüssel-Hashing.*
- `9b8c256` | **google-labs-jules[bot]** | `enforce lowercase-input invariant on GermanCompoundSplitter::decompose`
  *Erzwingung der Lowercase-Invariante in der deutschen Morphologie-Engine für deterministische Zerlegung.*
- `499682d` | **google-labs-jules[bot]** | `feat(graph): add pure-Rust Session-DAG and CheckpointGuard helper`
  *Implementierung des Pure-Rust Session-DAG für Verzweigungen im Agenten-Status (Grok-Pattern).*
- `f573619` | **google-labs-jules[bot]** | `feat(rag): implement Anthropic Contextual Retrieval pattern`
  *LLM-basierte Präfix-Generierung für Chunks vor der Indizierung zur Reduktion von Retrieval-Fehlern.*
- `f50c3b3` | **google-labs-jules[bot]** | `docs(rag): validate and confirm full implementation of RAG integration sprints`
  *Verifizierung und Dokumentation der RAG-Pipeline-Integration.*
- `c595bee` | **google-labs-jules[bot]** | `feat(mcp): implement stdio MCP server for Claude Desktop`
  *Bereitstellung der primären MCP-Schnittstelle (`memfuse_search`, `memfuse_insert`, `memfuse_get`, `memfuse_collections`).*
- `c49aacd` | **google-labs-jules[bot]** | `fix(checkpoint): prevent GC race and TxId collisions`
  *Schutz der Garbage Collection vor Race Conditions bei gleichzeitig aktiven Transaktionen.*

### 27. August 2026
- `969dd1d` | **google-labs-jules[bot]** | `fix(store): enhance LSM durability, concurrency safety, and format`
  *Erhöhung der Durability durch Verzeichnis-Fsyncs (`parent_dir.sync_all()`) und FFi/MVCC Concurrency Safety.*
- `f80fd9a` | **google-labs-jules[bot]** | `fix(memfuse-index): escape brackets in check_drift doc comment and format workspace`
  *Korrektur von Markdown-Formatierungsfehlern im Dokumentations-Kommentar von `memfuse-index`.*
- `1925004` | **google-labs-jules[bot]** | `feat(xtask): add xtask dev tool for documentation synchronization`
  *Einführung von `cargo xtask sync-docs` zur automatisierten Aktualisierung von `WORKING_STATE.md` und `ARCHITECTURE.md`.*
- `0e3d7bf` | **google-labs-jules[bot]** | `ci: upgrade Gate 5 to enforce sync-docs documentation drift check`
  *CI-Integration der Drift-Prüfung zur Vermeidung veralteter Architektur-Dokumente.*

### 28. August 2026
- `75d6ed9` | **google-labs-jules[bot]** | `feat(memfuse-graph): add bi-temporal validity axes to Edge and CsrGraph`
  *Erweiterung des CSR-Wissensgraphen um bi-temporale Zeitachsen (Validitäts- und Transaktionszeit).*
- `fe42626` | **google-labs-jules[bot]** | `fix(security): resolve WAL integrity key TOCTOU (F-07) and SIMD distance checks (F-08/F-09)`
  *Behebung von Time-of-Check to Time-of-Use Schwachstellen beim Erstellen von WAL-Integritätsschlüsseln.*
- `0695e55` | **google-labs-jules[bot]** | `chore(index,db): add TS timestamp to AGT-DB-004 and update AGT-INDEX-002`
  *Aktualisierung der ISO-Zeitstempel in Governance-Tags.*
- `df3ed80` | **google-labs-jules[bot]** | `refactor(core): audit error taxonomy, traits and tx_buffer`
  *Konsolidierung der Fehler-Hierarchie in `MemFuseError` und Staging-Kapazitätsgrenzen im Transaktionsbuffer.*
- `bc5610a` | **google-labs-jules[bot]** | `refactor(core): introduce CapabilityUnsupported error variant for trait defaults`
  *Hinzufügen der `CapabilityUnsupported` Fehler-Variante zur sauberen Behandlung nicht-implementierter Trait-Standards.*
- `9626b53` | **google-labs-jules[bot]** | `Unify MetadataFilter into FilterExpr and update docs`
  *Vereinheitlichung der Metadaten-Filterungs-DSL unter `FilterExpr`.*
- `4547237` | **google-labs-jules[bot]** | `refactor(db): rename compaction module to context_compaction`
  *Eindeutige Modulbenennung zur Unterscheidung zwischen LSM-STCS-Compaction und LLM-Kontextkompaktierung.*
- `fc25ca6` | **google-labs-jules[bot]** | `genericize AuditLog over StorageEngine default LsmStorage`
  *Generische Abstraktion von `AuditLog<S: StorageEngine>` zur Unterstützung flexibler Test-Engines.*
- `203af35` | **google-labs-jules[bot]** | `Remove deprecated ContextChunk::combined_text_for_indexing and sync docs`
  *Bereinigung veralteter API-Methoden.*
- `2a1313b` | **google-labs-jules[bot]** | `fix(graph): best-effort non-convergence behavior & log signals for PPR and Community Detection`
  *Best-Effort Rückgabe von Teilergebnissen bei Nicht-Konvergenz von Personalized PageRank und Community Detection mit `tracing::warn!` Signalierung.*
- `e10809b` | **google-labs-jules[bot]** | `feat(core,tauri,py): implement structured MemFuseErrorDto for IPC and FFI boundaries`
  *Einführung von `MemFuseErrorDto` (`kind`, `message`, `details`) für typsichere Fehlerübertragung über Tauri IPC und PyO3 FFI (ADR-028).*
- `a87fd15` | **google-labs-jules[bot]** | `feat(db): implement full 4-index 2PC transaction commit and rollback`
  *Vollständiger 2-Phase-Commit über HNSW, BM25, CSR-Graph und Metadaten-Indices mit atomarem Rollback.*
- `2b54f9d` | **google-labs-jules[bot]** | `docs: verify governance system hardening baseline (ADR-029)`
  *Verifizierung der Governance-Invarianten und Dokumentation von ADR-029.*
- `2dc334e` | **google-labs-jules[bot]** | `feat(store): implement WAL V3 format with tx_id HMAC binding`
  *Kryptographisch gehärtetes Write-Ahead-Log mit HMAC-SHA256 Bindung pro Transaktions-ID.*

### 29. August 2026
- `b03ec7f` | **google-labs-jules[bot]** | `Harden CheckpointGuard RAII safety and manifest atomicity`
  *Absicherung des RAII CheckpointGuards und atomare Speicherung des Snapshot-Manifests.*
- `575660f` | **google-labs-jules[bot]** | `fix(index): enforce SIMD preconditions, update AGT tags and sync docs`
  *Längenprüfungen vor Aufruf von SIMD-Distanzfunktionen zur Abwehr von Panics.*
- `9274dc6` | **google-labs-jules[bot]** | `fix(ci): synchronize documentation and fix CI gate checks`
  *Korrektur der CI-Gate-Skripte und Dokumentations-Abgleich.*
- `15077e6` | **google-labs-jules[bot]** | `Enforce AGT-GRAPH-001 TxId origin invariant via debug_assert`
  *Erzwingung der Transaktionsursprungs-Invariante im Wissensgraphen.*
- `40c3a23` | **google-labs-jules[bot]** | `fix(checkpoint,store): resolve AGT-CKPT-f3a1b2c4 and AGT-STORE-003`
  *Behebung kritischer Concurrency- und Isolation-Edge-Cases.*
- `000a4d3` | **google-labs-jules[bot]** | `feat: add sequence-based document TTL expiry reaper`
  *Implementierung des Hintergrund-Reapers für automatische TTL-Dokumentenlöschung nach Sequenznummern.*
- `83cc572` | **google-labs-jules[bot]** | `feat(router): implement memfuse-router crate for SLM context routing`
  *Einführung der SLM Context Routing Engine für dynamisches Prompt-Routing.*
- `31aed3a` | **google-labs-jules[bot]** | `Audit and enhance memfuse-embed and memfuse-ollama robustness`
  *Härtung der Ollama- und ONNX-Embedding-Integration gegen API-Timeout und Verbindungsabbrüche.*
- `ae157b5` | **google-labs-jules[bot]** | `fix(mcp): verify insert chunking, add prompt injection guard, zeroize sandbox & cap stdio rpc line size`
  *Umfassendes MCP Security Package: Schutz vor Prompt Injection, Pufferdeckelung und Speicherbereinigung.*
- `f8a9030` | **google-labs-jules[bot]** | `fix(tauri): eliminate startup panic and harden IPC ingestion security`
  *Absicherung der Desktop-App gegen Abstürze bei Start ohne konfigurierten Datenbank-Pfad.*
- `6d931e6` | **google-labs-jules[bot]** | `fix(memfuse-py): harden FFI boundaries, error mapping, and GIL concurrency`
  *Freigabe des Python GIL während rechenintensiver Suchen und Mapping auf PyErr-Objekte.*
- `a3f363e` | **google-labs-jules[bot]** | `fix(xtask,bench): resolve check-consistency failure and migration benchmarks dimension mismatch`
  *Korrektur von Dimensionsungleichheiten in Benchmarks.*
- `9bcf07f` | **google-labs-jules[bot]** | `refactor(memfuse-db): decouple collection.rs and harden TxId allocation`
  *Dekopplung von `collection.rs` in modulare Submodule (`crud`, `search`, `maintenance`, `relate`).*
- `37aa6a3` | **google-labs-jules[bot]** | `Add MemoryType enum and insert_typed public API`
  *Kognitive Klassifikation von Dokumenten in `Episodic`, `Semantic`, `Procedural` und `Working` Memory (ADR-041).*
- `2c263bf` | **google-labs-jules[bot]** | `feat(core/db): implement Zettelkasten memory links and supersedes displacement`
  *A-MEM Zettelkasten Pattern mit Verknüpfungen und Ersetzungs-Semantik für veraltete Erinnerungen.*
- `0dea264` | **google-labs-jules[bot]** | `refactor(db): modularize collection.rs into submodules`
  *Aufteilung der großen `collection.rs` Datei für bessere Wartbarkeit.*
- `2c6bf35` | **google-labs-jules[bot]** | `Harden memfuse-store against silent errors and invalid inputs`
  *Propagation aller I/O-Fehler beim Verzeichnis-Fsync und Vermeidung stummer Resultat-Ignorierung (`let _ =`).*
- `76c1eeb` | **google-labs-jules[bot]** | `Fix Tauri path traversal, ingestion limits, and silent IO check in HNSW`
  *Behebung von Pfad-Traversierungs-Risiken im Tauri File Picker und Ingestion-Limits.*
- `2bf754e` | **google-labs-jules[bot]** | `perf: zero-copy scan_prefix and clone reduction in lsm & collection`
  *Performance-Optimierung durch Zero-Copy Prefix Scanning im LSM-Store.*
- `cc1e5e9` | **google-labs-jules[bot]** | `harden(crypto): enforce non-empty input validation on KeyManager & WalHmac`
  *Eingabe-Validierung gegen leere Schlüssel und Payloads.*
- `c17b7c5` | **tfufuz1** | `Fix/memfuse agent state and audit integrity 4394097478157732988 (#1018)`
  *Sicherstellung der Integrität von Agenten-Workflow-Sitzungen und Audit-Logs.*

### 30. August 2026
- `097a134` | **google-labs-jules[bot]** | `fix(store): correct wal tail truncation condition in batch replay`
  *Korrektur der Abbruchbedingung beim Replay beschädigter WAL-Dateien.*
- `a82f0dc` | **google-labs-jules[bot]** | `docs: document ADR-041 for cognitive memory type classification (MemoryType)`
  *Architekturentscheidung für kognitive Gedächtnistypen im System.*
- `d900476` | **google-labs-jules[bot]** | `deprecate Collection::next_tx in favor of allocate_tx`
  *Ersetzung veralteter Transaktions-Allokation durch unfehlbare/fehlerabfangende API.*
- `fb1f918` | **google-labs-jules[bot]** | `feat(graph): PprConfig warn_on_non_convergence, community proptests & xtask gate fix`
  *Einführung von Eigenschafts-Tests (Proptests) für Graph-Community-Detection.*
- `a6034be` | **google-labs-jules[bot]** | `refactor(store): fix batch WAL decryption loop duplication`
  *Deduplizierung des WAL-Entschlüsselungscodes.*
- `bbdf3f2` | **google-labs-jules[bot]** | `fix(lsm): mask TOMBSTONE_BIT in rollback_to_tx`
  *Korrektes Maskieren des Tombstone-Bits bei Transaktions-Rollbacks im LSM-Tree.*
- `bdb3518` | **google-labs-jules[bot]** | `Fix SSTable compaction crash safety via Write-Temp-Then-Rename`
  *Crash-sichere Compaction: Schreiben in `.sst.tmp` Datei und atomares `tokio::fs::rename` nach `file.sync_all()` (ADR-042).*
- `2ff1fa4` | **google-labs-jules[bot]** | `perf(index): move NaN query check to entry point`
  *Vorzeitiger Abbruch bei NaN-Eingabevektoren am Einstiegspunkt der HNSW-Suche.*
- `6a40bca` | **google-labs-jules[bot]** | `fix(mcp): harden memfuse-mcp protocol and input validation`
  *Validierung aller Parameter im MCP JSON-RPC Interface.*
- `c800c49` | **google-labs-jules[bot]** | `harden(agent): prevent silent errors and resource exhaustion in memfuse-agent`
  *Ressourcenbegrenzung und explicit Result-Unwrapping in Agenten-Schleifen.*
- `390201a` | **google-labs-jules[bot]** | `Harden memfuse-ollama against silent errors and resource bounds`
  *Härtung des Ollama-Clients gegen unbegrenzte HTTP-Antworten.*
- `c028100` | **google-labs-jules[bot]** | `harden(memfuse-text, memfuse-db): add input guards and batch boundary checks`
  *Grenzbereich-Validierung bei Batch-Operationen im Invertierten Index.*
- `b084da5` | **google-labs-jules[bot]** | `fix(ci): resolve context-gates review-coverage failure and compilation issues`
  *Behebung von CI-Fehlern bezüglich Review-Coverage-Prüfungen.*
- `43eed69` | **google-labs-jules[bot]** | `harden(checkpoint): input validation, resource caps, lock hierarchy & REVIEW-PASS`
  *Konsolidierung der Sperr-Hierarchien zur Deadlock-Vermeidung im Checkpoint-Manager.*
- `d7ecd28` | **google-labs-jules[bot]** | `feat(xtask): extend check-consistency with README, AGENTS.md, and ADR checks`
  *Erweiterung des xtask Konsistenz-Checkers um Validierung von README Crate-Zahlen und AGENTS.md Existenz.*
- `37a2fab` | **google-labs-jules[bot]** | `harden(agent): add zero-panic deprecations, input validation, and review pass tags`
  *Zero-Panic Garantie in `memfuse-agent` durch vollständiges Entfernen ungeschützter `.unwrap()` Aufrufe in Production-Pfade.*
- `6b540a7` | **google-labs-jules[bot]** | `refactor(core): fulfill ANCHOR[TEST:CORE-001], consolidate headers & sync docs`
  *Erfüllung der Core-Test-Anforderungen und Synchronisation der Arbeitsstände.*

### 31. August 2026 (Heute)
- `5b067ad` | **tfufuz1** | `refactor(index): audit and clean up memfuse-index code quality (#1150)`
  *Umfassendes Audit und Bereinigung von `memfuse-index`: Infallible Float-Konvertierungen (`f32::from`), Inlined Format Arguments, Validierung von NaN/Inf Query-Vektoren in `HnswIndex::search`, Aktualisierung der Session-Hashes.*

---

## 3. Subsystem- & Crate-Entwicklung (Layer 0 bis 4)

Das Repository ist als modularer Workspace aufgebaut. Die Historie spiegelt die gezielte Weiterentwicklung jeder Schicht wider:

```
┌─────────────────────────────────────────────────────────────────┐
│ Layer 4: Desktop App & MCP Server                               │
│  - memfuse-tauri (Tauri Desktop App Shell)                      │
│  - memfuse-mcp (JSON-RPC 2.0 MCP Server & Stdio Sandbox)        │
├─────────────────────────────────────────────────────────────────┤
│ Layer 3: Client Interfaces & RAG Augmentation                   │
│  - memfuse-py (PyO3 Python FFI Bindings)                        │
│  - memfuse-ollama (Lokales LLM / Embedding Backend)             │
│  - memfuse-agent (Persistent Agent Workflow Engine)             │
│  - memfuse-embed (In-Process ONNX Reranking & Embeddings)       │
│  - memfuse-router (SLM Context Routing Engine)                  │
├─────────────────────────────────────────────────────────────────┤
│ Layer 2: Core Database Orchestrator                             │
│  - memfuse-db (4-Signal Fusion, MultiStep Engine, Compactor)    │
├─────────────────────────────────────────────────────────────────┤
│ Layer 1: Storage & Specialized Indices                          │
│  - memfuse-store (LSM-Tree, WAL V3, MemTable Sharding)         │
│  - memfuse-index (HNSW, DiskANN, Quantisierung, SIMD)           │
│  - memfuse-text (BM25, Invertierter Index, Morphologie)         │
│  - memfuse-crypto (AES-256-GCM-SIV, HKDF, Anti-Tamper WAL)      │
│  - memfuse-graph (CSR-Graph, Bi-temporal Axes, Session-DAG)    │
│  - memfuse-checkpoint (MVCC Snapshot-Pinning, CheckpointGuard) │
├─────────────────────────────────────────────────────────────────┤
│ Layer 0: Core Abstractions & Data Types                         │
│  - memfuse-core (MemFuseError, Domain Types, FilterExpr, Traits)│
└─────────────────────────────────────────────────────────────────┘
```

### Layer 0: `memfuse-core`
- Einführung des kanonischen `MemFuseError` und `MemFuseErrorDto` für FFI/IPC.
- Modellierung von `FilterExpr` (Unified Metadata Filter DSL) und `MemoryType` (`Episodic`, `Semantic`, `Procedural`, `Working`).
- Transaktionaler Staging-Puffer (`TxBuffer`) mit strikter Kapazitätsdeckelung (`max_ops_per_tx = 10_000`) zum Schutz vor OOM.

### Layer 1: Storage & Spezialisierte Indizes
- **`memfuse-store`**: Implementierung des WAL V3 Formats mit HMAC-SHA256 Transaktionsbindung. Crash-sichere Compaction via `.sst.tmp` Schreiben und atomares Rename (ADR-042). AHash-basiertes Sharding für MemTables.
- **`memfuse-index`**: HNSW Vektorindex mit SIMD-beschleunigten Metriken (Cosine, L2, Dot Product). DiskANN Out-of-Core Graphindex mit In-Memory Cache und POSIX Atomic Rename für Indexdateien. Skalare 8-Bit Quantisierung (SQ8).
- **`memfuse-text`**: Invertierter Index für BM25 mit deutscher Morphologie (Umlaut-Normalisierung, Komposita-Zerlegung) und Tombstone-Update-Semantik.
- **`memfuse-crypto`**: Authentifizierte AES-256-GCM-SIV Verschlüsselung, `OsRng` Nonce-Garantie, `Zeroize`-Disziplin für flüchtige Schlüssel und konstanter WAL-HMAC-Verifier.
- **`memfuse-graph`**: CSR-Graph mit bi-temporalen Zeitachsen (Validitäts- und Transaktionszeit). Personalized PageRank (PPR) mit Power Iteration, Community Detection via Label Propagation und Pure-Rust `SessionBranchTree` für Konversations-Verzweigungen.
- **`memfuse-checkpoint`**: RAII-basierter `CheckpointGuard` für atomare MVCC-Snapshots.

### Layer 2: `memfuse-db`
- Orchestrierung des **4-Signal-Hybrid-Retrieval** (HNSW + BM25 + CSR-Graph + Metadaten) via Reciprocal Rank Fusion (RRF).
- **Full 2-Phase Commit (2PC)**: Atomares Transaktionsmanagement über alle 4 Indizes hinweg mit automatischer Kompensation bei Teilfehlern.
- **Multi-Step Retrieval Engine**: Iteratives Query-Rewriting (OpenAI o-series Pattern) für komplexe Abfragen.
- **Context Compaction & Reaper**: LLM-basierte Zusammenfassung und automatischer Expiry Reaper für TTL-abgelaufene Dokumente.

### Layer 3: FFI, Models & Agenten
- **`memfuse-py`**: PyO3-Bindings mit automatischer GIL-Freigabe bei zeitintensiven Operationen und Konvertierung von `MemFuseErrorDto` in strukturierte Python-Exceptions.
- **`memfuse-ollama`**: HTTP-Client mit Batch-Embedding-Unterstützung (`/api/embed`), Automatischer Fallback und Anthropic Contextual Retrieval Präfixerstellung.
- **`memfuse-agent`**: Hintergrund-Workflow-Engine mit Event-Loop (`EventSource`), State Checkpointing und speicherbeschränkten Event-Queues.
- **`memfuse-embed`**: Optionaler in-process ONNX Session Pool für Cross-Encoder Reranking.
- **`memfuse-router`**: SLM-basiertes Kontext-Routing zur dynamischen Modell-Auswahl.

### Layer 4: Schnittstellen & Desktop App
- **`memfuse-mcp`**: Stdio JSON-RPC 2.0 MCP-Server für Claude Desktop und Agenten mit MCP-Sandbox (Zero-Trust Tool Isolation, Zeroize von sensitiven Outputs, Prompt-Injection Schutz).
- **`memfuse-tauri`**: Pure Desktop Shell für Windows, macOS und Linux mit HTML-Escaping (XSS-Schutz) und abgesicherter IPC-Ingestion.

---

## 4. Governance & Qualitäts-Sicherung

Die Projekt-Historie zeichnet sich durch ein streng durchgesetztes Governance-System aus:

1. **Architecture Decision Records (ADRs)**: Strikte Einhaltung von Vorgaben bezüglich MVCC Isolation (ADR-012/ADR-043), MCP Stdio-Kommunikation (ADR-010), Error Propagation via DTOs (ADR-028) und Governance System Hardening (ADR-029).
2. **Inline Code Tags & Review Passes**: Verwendung von `ANCHOR[...]`, `AI-TAG[...]` und `REVIEW-PASS[...]` Annotationen mit ISO-8601 Zeitstempeln (`TS:2026-08-30T...`) und Session-Hashes.
3. **Automatisierte CI Enforcement Gates**:
   - `cargo xtask check-consistency`: Überprüft Workspace-Crate-Anzahlen, `AGENTS.md` Abdeckung, ADR-Eindeutigkeit.
   - `cargo xtask sync-docs`: Verhindert Drift zwischen Quellcode-Annotationen und Dokumentationsdateien (`WORKING_STATE.md`, `ARCHITECTURE.md`).
   - `context-gates.yml`: Verhindert ungelöste `CRITICAL` Code Smells und prüft die Gültigkeit von Anchor-Tags.

---

## 5. Statistische Kennzahlen

- **Aktive Workspace Crates**: 15 Crates (Layer 0 bis Layer 4)
- **Commits insgesamt**: >210 Merges und Direkt-Commits
- **Verteilte Autoren**: `google-labs-jules[bot]`, `tfufuz1`
- **Programmiersprache**: 100% Rust (mit Tauri UI HTML/JS Frontend & PyO3 Python-Interface)
- **Sicherheit & Zero-Panic Policy**: Volle Beseitigung aller unkontrollierten `.unwrap()` Aufrufe in Produktivpfaden (abgesichert via `// unwrap allowed` mit nachgewiesenen Invarianten).

---

*Ende der Projekthistorie.*
