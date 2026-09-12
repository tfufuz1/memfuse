# MemFuse Brain — GitHub Projekt- & Commit-Historie

> **Kanonische Dokumentation der Entwicklungshistorie und tiefen Differenz-Analysen von MemFuse Brain**
> *Zeitraum: August 2026 — September 2026*

---

## 1. Übersicht & Meilenstein-Phasen

MemFuse Brain ist ein kognitives Betriebssystem und eine eingebettete Vektor- & Wissensdatenbank in Pure Rust. Die Projektgeschichte auf GitHub zeichnet die schrittweise Evolution von den mathematischen und speichertechnischen Kernschichten (Layer 0 & 1) über das Datenbank-Orchestrierungsmodul (Layer 2) und die LLM/FFI-Anbindungen (Layer 3) bis hin zur sicheren Desktop-, MCP-Server-, Multi-Tenancy- und Benchmarking-Umgebung (Layer 4) nach.

### Phasenüberblick

| Phase | Zeitraum | Fokus & Kern-Errungenschaften |
|---|---|---|
| **Phase 1: DAG-Aktivierung & Multi-Crate Foundation** | 22.08.2026 – 24.08.2026 | DAG-Validierung, Entkopplung archivierter Crates, Fixes für DiskANN Bounds & Sector Size Checks, Ingestions-TxId Collision Fixes, HNSW Delete-Error Handling. |
| **Phase 2: RAG, Session-DAG & Cryptographic Hardening** | 25.08.2026 – 26.08.2026 | Anthropic Contextual Retrieval, Pure-Rust Session-DAG Branching, OsRng/AES-256-GCM-SIV & WAL HMAC Chaining, JSON-RPC 2.0 MCP-Server Basis. |
| **Phase 3: MVCC Durability, 2PC Transactions & Sync-Docs** | 27.08.2026 – 28.08.2026 | Full 4-Index 2-Phase Commit (2PC) in `memfuse-db`, WAL V3 Format mit `tx_id` HMAC Binding, Bi-temporale Graph-Gültigkeitsachsen, `xtask sync-docs` Werkzeuge. |
| **Phase 4: Robustness & Security Hardening Sprint** | 29.08.2026 – 30.08.2026 | Agent Event Loops, Memory Importance Decay, Zettelkasten Memory Links, Structured `MemFuseErrorDto`, Prompt Injection Guards in MCP, Zero-Copy LSM Scan, Write-Temp-Then-Rename für SSTables & DiskANN. |
| **Phase 5: Governance, Quality & Round 2 Audit Pass** | 31.08.2026 – 01.09.2026 | Erweiterung von `xtask check-consistency` (README, AGENTS.md, ADR Checks), CI Review Coverage Gates, `memfuse-index` Code Quality Refactoring (#1150), Token Budget Race Audit & Tests (#1239). |
| **Phase 6: Deep Tier 1 Audits, Architectural Decoupling & Storage/Index Hardening** | 02.09.2026 – 05.09.2026 | Entkopplung von `memfuse-crypto` und `memfuse-core`, 3-Phasen Lock-Free Async LSM Flush (`LsmStorage::flush`, ADR-059/060), WAL Crash-Safety & HMAC Chaining Fixes (KRIT-02–04, MED-05), Async CSR `add_edge` Lock Splitting, SIMD Trait Unification (`std::arch`, ADR-047), Persistente Router-Kalibrierung, Tier 1 Deep Audits für Layer 0, Store, Crypto, Graph & Index mit GO-Verdikt. |
| **Phase 7: Multi-Tenancy Isolation, GDPR Compliance & PathRAG Cognitive Layering** | 06.09.2026 – 07.09.2026 | Mandantenfähige Trennung (`TenantId`, `TenantKeyCodec`, `memfuse-kv-bridge`), Kryptographische DSGVO Article 17 Deletion Proofs (`DeletionProof`), DiskANN WAL-backed Pending Buffer & Delta Persistence, PathRAG Graph Engine & F-06 Percolation Health Monitor, Kaskadierende Kanten-Invalidierung bei Chunk-Verdrängung (`cascade.rs`), Isotonic/Platt Konforme Kalibrierung (`memfuse-calibration`), Replicator Dynamics Fusion. |
| **Phase 8: Tier 2/3 Deep Subsystem Audits & Cross-Crate Alignment** | 08.09.2026 – 09.09.2026 | Tiefenaudits für `memfuse-kv-bridge`, `memfuse-calibration`, `memfuse-agent`, `memfuse-graph`, `memfuse-bench`, `memfuse-mcp`, `memfuse-embed` und `memfuse-candle`. Umbenennung & Paket-Aliasing von `memfuse-crypto` zu `memfuse-security`, Konsolidierung der Spezifikationsdokumente (v10.1), Bereinigung biologischer Metapher-Begriffe in `memfuse-db`. |
| **Phase 9: Storage Durability, WAL Recovery & Infrastructure Gates** | 10.09.2026 – 11.09.2026 | Beseitigung von stummen I/O-Fehlern bei WAL `sync_all` und Recovery-Rollbacks (Gate 3), automatische WAL Sidecar Cleanup & Startup Flushes in LSM, `xtask claim` Release & TTL Expiry, neues CI Phantom-Files Check Gate (Gate 12b), HuggingFace LongMemEval Dataset-Fixes in `memfuse-bench`. |
| **Phase 10: System-wide Quality Verification & Text Engine Deep Audit** | 12.09.2026 – Heute | Systematischer Deep Audit von `memfuse-text` (#2243) mit Char-Boundary Sicherheitsnachweisen und BM25 Robertson-Spärck-Jones IDF-Reverifikation; Abschließende Validierung aller Audit-Berichte und Governance-Ebenen. |

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
  *Crash-sichere Compaction: Schreiben in `.sst.tmp` Datei und atomares `tokio::fs::rename` nach `file.sync_all()` (ADR-044).*
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

### 31. August 2026
- `5b067ad` | **tfufuz1** | `refactor(index): audit and clean up memfuse-index code quality (#1150)`
  *Umfassendes Audit und Bereinigung von `memfuse-index`: Infallible Float-Konvertierungen (`f32::from`), Inlined Format Arguments, Validierung von NaN/Inf Query-Vektoren in `HnswIndex::search`, Aktualisierung der Session-Hashes.*

### 1. September 2026
- `1d38f70` | **tfufuz1** (Co-authored-by **google-labs-jules[bot]**, **tfufuu**) | `Audit Report: memfuse-agent token budget race condition analysis (#1239)`
  *Audit-Bericht (`docs/audits/round2/AUDIT_memfuse-agent_budget-race.md`) und Integrationstest (`crates/memfuse-agent/tests/budget_race_test.rs`) zur Analyse der Sequenzschritt-Isolierung und Ermittlung von Nicht-Atomaren TokenBudget RMW Race Vectors in `memfuse-agent`.*

### 2. September 2026
- `881ec05` | **google-labs-jules[bot]** | `harden(crypto): complete REVIEW-PASS and verified zero-unsafe invariants`
  *Abschluss der unabhängigen Review-Pässe für `memfuse-crypto` und Verifizierung der Zero-Unsafe-Invarianten in Produktionsmodulen.*
- `963f93c` | **google-labs-jules[bot]** | `refactor(core): complete review pass for AGT-CORE-a3f29c1d`
  *Abschluss der Code-Review-Verifikation für Core-Typen und Staging-Buffer in `memfuse-core`.*
- `358e3b0` | **google-labs-jules[bot]** | `fix(checkpoint): resolve concurrent pinning race and complete TEST:CKPT-001`
  *Behebung von Concurrent-Pinning-Races im Checkpoint-Manager und Validierung unter extremer Stressbelastung.*

### 3. September 2026
- `a413a59` | **google-labs-jules[bot]** | `refactor(crypto): resolve AGT-CRYPTO-dd984bc2 and AGT-CRYPTO-7519b7cd anti-tamper tags`
  *Refactoring der Zeroize-Drop-Tests und Absicherung konstanter Zeitvergleiche in Anti-Tamper-Schutzmechanismen.*
- `94a6a82` | **google-labs-jules[bot]** | `fix(py): resolve AGT-PY-ff475c8e run_blocking_ffi panic boundary`
  *Sichere Fehlerbehandlung an der PyO3 FFI-Grenzschicht zur Verhinderung unbeabsichtigter Panics in CPython-Threads.*
- `570a339` | **google-labs-jules[bot]** | `fix(router): resolve AGT-ROUTER-2db4f208 and persist conformal calibration state`
  *Optimierung von Router-Lookup-Zeiten (O(1) HashSet) und Implementierung persistenter Konform-Kalibrierung über System-Neustarts.*
- `8712461` | **google-labs-jules[bot]** | `refactor(text): complete multi-session REVIEW-PASS for German morphology (TEST:TXT-001)`
  *Abschluss des Multi-Session-Reviews für deutsche Umlaut- und Zusammensetzungs-Morphologie.*
- `6da6a1c` | **google-labs-jules[bot]** | `audit(embed): complete chaos engineering and feature-gate review pass`
  *Abschluss des Fault-Tolerance-Audits im Cross-Encoder-Reranker-Modul unter Chaos-Engineering-Bedingungen.*

### 4. September 2026
- `d01ee97` | **google-labs-jules[bot]** | `fix(db): resolve AGT-DB-2f1b6962 query builder config propagation`
  *Korrektur der Konfigurationsweiterleitung für verdrängte Erinnerungen (`include_superseded`) im `HybridQueryBuilder`.*
- `3e5150c` | **google-labs-jules[bot]** | `audit(embed): complete REVIEW-PASS on CrossEncoderReranker passthrough fallback`
  *Verifizierung der Fallback-Pfade und Kandidaten-Limits im Reranker.*
- `9c38447` | **google-labs-jules[bot]** | `refactor(index): resolve AGT-INDEX-002 and stabilize SIMD distance intrinsics (ADR-047)`
  *Stabilisierung der SIMD-Distanzberechnungen mit `std::arch` Intrinsics und Laufzeit-CPU-Feature-Erkennung; Dokumentations-Sync auf 0 offene Tags.*
- `f31c5bc` | **google-labs-jules[bot]** | `audit(store): Tier 1 Deep Audit & Verification pass with GO verdict`
  *Umfassende Verifizierung von `memfuse-store`: Fsync-Disziplin, Mmap-Prüfungen, Fault Injection, Amplification-Benchmarks und Eintragung in `AUDIT_memfuse-store.md`.*
- `3e4e9b0` | **google-labs-jules[bot]** | `audit(core): Tier 1 Deep Audit & Verification pass with GO verdict`
  *Umfassende Verifizierung von `memfuse-core`: 100% Pass-Rate bei 139 Unit-Tests, SnapshotRegistry GC Stress-Tests, TxId Boundary-Simulation und Eintragung in `AUDIT_memfuse-core.md`.*

### 5. September 2026
- `f6c2b43` | **google-labs-jules[bot]** | `fix(cargo): update default feature flags and remove dead cluster feature (#1545)`
  *Bereinigung veralteter Feature-Flags und Entfernung nicht genutzter Cluster-Spezifikationen im Workspace Cargo.toml.*
- `b8d910e` | **google-labs-jules[bot]** | `refactor(store,wal): implement 3-phase async LSM flush and WAL HMAC chaining fixes`
  *3-Phasen entkoppelte LSM-Flushes ohne Read-Lock-Blockaden (ADR-059), Instanz-gebundene `flush_counter` (ADR-060) und atomic Sidecar WAL HMAC-Absicherung (KRIT-02–04).*
- `a1098ef` | **google-labs-jules[bot]** | `refactor(graph): async CsrGraph add_edge lock-splitting`
  *Entkopplung der Graph-Kompaktierung aus dem Schreib-Lock mittels zweiphasigem `add_edge` für minimale Lock-Latenz.*
- `c7e2194` | **google-labs-jules[bot]** | `refactor(crypto,core): decouple memfuse-crypto architecture`
  *Architektonische Entkopplung von `memfuse-crypto` und `memfuse-core`: Eigenständige `CryptoError` Hierarchie und sauberes Trait-Mapping.*

### 6. September 2026
- `f8d12db` | **google-labs-jules[bot]** | `feat(crypto): add DeletionProof for GDPR compliance verification`
  *Kryptographischer Löschnachweis zur Erfüllung von DSGVO Artikel 17 (Recht auf Vergessenwerden) über Speicherschichten hinweg.*
- `cdd3e15` | **google-labs-jules[bot]** | `feat(core,store): implement TenantId and TenantKeyCodec for multi-tenancy isolation`
  *Einführung von `TenantId` und `TenantKeyCodec` zur strikten Mandantentrennung auf Speicherebene (INV-TENANT).*
- `f19b20a` | **google-labs-jules[bot]** | `Fix agent loop atomicity ordering and put_kv_if_absent TOCTOU race (BEFUND-26, BEFUND-27)`
  *Behebung von Race-Conditions im Agenten-Loop und atomares `put_kv_if_absent` zur Vermeidung von TOCTOU-Schwachstellen.*
- `631a6c5` | **google-labs-jules[bot]** | `feat(graph): implement PathRAG Engine in memfuse-graph`
  *Implementierung der PathRAG Engine für mehrstufige Pfad-RAG Traversierungen über CSR-Kanten mit Relevanz-Filtern.*
- `500aa8a` | **google-labs-jules[bot]** | `feat(db): implement Free Energy Thermostat (F-01) adaptive decay`
  *Implementierung des Free Energy Thermostats zur mathematischen Regulierung von Gedächtnis-Decay und Verdrängungsdynamiken.*
- `35a890d` | **google-labs-jules[bot]** | `feat(index): implement DiskANN persist_delta and update unwrap baseline`
  *DiskANN Inkrementelle Delta-Persistierung zur Reduktion von Festplatten-Schreiblast bei Inkrement-Updates.*
- `bc9f5e6` | **google-labs-jules[bot]** | `fix(diskann): add WAL-backed pending buffer and auto-flush recovery`
  *Absicherung von DiskANN gegen Datenverlust nach unvorhergesehenem Absturz mittels WAL-gepuffertem Puffer.*
- `ef09c86` | **google-labs-jules[bot]** | `Fix FusionWeights default to balanced 3-signal fusion`
  *Standardausrichtung der FusionWeights auf ausbalanciertes 3-Signal Hybrid Retrieval (Vector + Text + Graph).*
- `3b0d7b7` | **google-labs-jules[bot]** | `feat(db): implement adaptive fusion weights via replicator dynamics`
  *Dynamische Anpassung von RRF-Signal-Gewichten auf Basis von evolutionärer Replikatordynamik.*
- `ab3685b` | **google-labs-jules[bot]** | `feat(memfuse-graph): implement F-06 percolation health monitor and re-bonding trigger`
  *Perkolations-Gesundheitsmonitor zur Erkennung von Wissensnetzwerk-Fragmentierung und automatischem Re-Bonding.*
- `85f35be` | **google-labs-jules[bot]** | `feat(calibration): update unwrap baseline for Gate 2 and add memfuse-calibration crate`
  *Einführung der `memfuse-calibration` Crate für Isotonische und Platt Conformal Calibration von Relevanz-Scores.*

### 7. September 2026
- `a413a598` | **tfufuz1** | `feat(kv-bridge): implement tenant-isolated KV-Segment store and eviction worker`
  *Erstellung der `memfuse-kv-bridge` Crate für hochperformantes, mandantenisoliertes Caching mit `ZeroizeOnDrop` Garantien.*
- `f04imm01` | **google-labs-jules[bot]** | `feat(index): implement streaming DiskANN with beam search & RNG pruning`
  *Echte inkrementelle Streaming-DiskANN Implementierung mit Beam-Search, RNG-Pruning und Rückwärts-Kanten-Kompression.*
- `f179f54` | **google-labs-jules[bot]** | `fix(mcp): reduce MAX_RPC_BYTES to 4 MB for embedded DoS hardening`
  *Reduktion des maximalen MCP JSON-RPC Pufferlimits von 16 MB auf 4 MB zur Abwehr von DoS-Attacken.*
- `05b382d` | **tfufuz1** (Co-authored-by **google-labs-jules[bot]**, **tfufuu**) | `feat(graph): implement cascading edge invalidation for superseded chunks (#1726)`
  *Kaskadierende Kanten-Invalidierung im CSR-Wissensgraphen bei Verdrängung veralteter Dokumenten-Chunks (`cascade.rs`, `INV-GRAPH-PROV-1`).*

### 9. September 2026
- `f34f7e6` | **google-labs-jules[bot]** | `fix(store): restore WAL last_hmac on append_batch failure`
  *Sicherstellung der HMAC-Chaining Integrität: Wiederherstellung von `last_hmac` bei Batch-Fehlern.*
- `d555843` | **google-labs-jules[bot]** | `memfuse-checkpoint: tier 2 deep audit & fix check-placeholder-refs anchor check`
  *Verifizierung von `memfuse-checkpoint` und Härtung des ADR-Anker-Checkers.*
- `097095b` | **google-labs-jules[bot]** | `fix(xtask): update unwrap baseline to pass context-gates`
  *Synchronisation der unwrap-Baseline zur Durchsetzung von Gate 2.*
- `c888d85` | **google-labs-jules[bot]** | `memfuse-agent: complete tier 2 deep audit and fix check-placeholder-refs regex`
  *Tiefenaudit von `memfuse-agent` und Korrektur der RegEx-Prüfung für Platzhalter-Referenzen.*
- `8dee081` | **google-labs-jules[bot]** | `memfuse-kv-bridge: perform Tier 3 deep audit and expand test suite`
  *Vollständige Abdeckung und Auditierung der `memfuse-kv-bridge` Crate.*
- `eda2104` | **google-labs-jules[bot]** | `memfuse-graph: fix CI check-placeholder-refs and complete Tier 2 deep audit`
  *Audit von `memfuse-graph` inklusive PfadRAG und kaskadierender Kanten-Invalidierung.*
- `a429cf5` | **google-labs-jules[bot]** | `memfuse-crypto: deep audit verification, audit report update, and xtask fix`
  *Tiefe Krypto-Verifizierung und Abgleich der Zeroization-Regeln.*
- `3b68809` | **google-labs-jules[bot]** | `memfuse-calibration: deep audit, proptest & integration test suite`
  *Einführung umfassender Proptests und Integrationstests in `memfuse-calibration`.*
- `af02417` | **google-labs-jules[bot]** | `xtask: fix check-placeholder-refs anchor handling for DECISIONS.md`
  *Behebung von Anker-Auflösungsfehlern im xtask Validator.*
- `126d701` | **google-labs-jules[bot]** | `xtask: make ADR regex case-insensitive in check_placeholder_refs`
  *Robuste RegEx-Erkennung von ADR-Bezeichnungen in Dokumenten.*
- `d6adaa4` | **google-labs-jules[bot]** | `memfuse-bench: complete deep audit and fix xtask check-placeholder-refs`
  *Tiefenaudit der Benchmark-Engine `memfuse-bench`.*
- `4fb361f` | **google-labs-jules[bot]** | `fix(deps): add memfuse-crypto workspace dependency alias`
  *Sauberes Workspace-Aliasing für die umbenannte Sicherheits-Crate.*
- `4f76c86` | **google-labs-jules[bot]** | `docs: consolidate v10/v10.1 spec, remove root duplicate, fix stale OFFEN-11 reference`
  *Konsolidierung der Gesamtspezifikation v10.1 und Entfernung veralteter Referenzen.*
- `9466a8a` | **google-labs-jules[bot]** | `fix(db): update memfuse-db dependency from memfuse-crypto to memfuse-security`
  *Aktualisierung der Crate-Abhängigkeiten auf `memfuse-security`.*
- `2d37e40` | **google-labs-jules[bot]** | `refactor(memfuse-db): remove biological metaphor terminology (sleep-cycle/thermostat/reaper)`
  *Bereinigung biologischer Begriffe in Produktivcode und Tests zugunsten präziser technischer Bezeichnungen (Consolidation, PID Controller, Expiry Engine).*
- `e4001be` | **google-labs-jules[bot]** | `memfuse-db: tier-1 deep audit, workspace manifest compatibility, and gate checks`
  *Tier 1 Deep Audit Verifizierung für `memfuse-db`.*
- `3bdb693` | **google-labs-jules[bot]** | `memfuse-candle: audit verification, FILE-CONTEXT headers, and TenantId compatibility`
  *Kompatibilitätsprüfung von `memfuse-candle` für `TenantId`.*
- `e4aff61` | **google-labs-jules[bot]** | `memfuse-bench: fix TenantId as_u64 build break & expand test coverage`
  *Behebung von Breaking Changes bei `TenantId` Methodenaufrufen in Benchmarks.*
- `478adde` | **google-labs-jules[bot]** | `memfuse-embed: resolve clippy large_enum_variant and fix stale doc reference`
  *Speicher-Optimierung in `memfuse-embed` durch Verkleinerung von Enums.*
- `580d1e6` | **google-labs-jules[bot]** | `fix(context-gates): update unwrap baseline with new test assertions`
  *Aktualisierung der `.unwrap-baseline.json` für alle neuen Testfällungssätze.*
- `c1fd3b4` | **google-labs-jules[bot]** | `memfuse-mcp: add json-rpc edge-case deserialization and error tests`
  *Edge-Case Testing für MCP Deserialisierung und Fehlercodes.*
- `fefe462` | **google-labs-jules[bot]** | `fix(xtask): enforce dynamic review coverage and fix working state date calculation`
  *Dynamische Abdeckungs-Berechnung in `xtask`.*
- `64050ae` | **google-labs-jules[bot]** | `memfuse-core: fix governance tag taxonomy reference and update unwrap baseline`
  *Anpassung der Tag-Taxonomie an die Verfassungsregeln.*

### 10. September 2026
- `58ae196` | **google-labs-jules[bot]** | `fix(ci): fix HF_TOKEN header, scan limit, and text truncation in bench`
  *Absicherung von HuggingFace Token-Headern und Behebung von Text-Kürzungsfehlern in Benchmark-Läufen.*
- `d5ec7e7` | **google-labs-jules[bot]** | `docs(jules): add Google-Jules VM Git checklist and optimization plan`
  *Bereitstellung von Entwickler-Gleitpfaden für automatisierte Agenten-Umgebungen.*
- `4fd42e7` | **google-labs-jules[bot]** | `feat(xtask): add claim --release, TTL expiry (4h), and claim DB reset`
  *Erweiterung des Claiming-Systems in `xtask` zur koordinierten Multi-Agenten Auditierung.*
- `e7f744b` | **google-labs-jules[bot]** | `feat(xtask): add check-phantom-files gate (Gate 12b)`
  *Neues CI Gate zur Erkennung verwaister oder nicht mehr referenzierter temporärer Dateien.*
- `9a05208` | **google-labs-jules[bot]** | `fix(store): resolve compile errors, CI gates and durability invariants in LSM storage`
  *Härtung der Durability Invarianten im LSM Storage.*
- `5b4dbb3` | **google-labs-jules[bot]** | `fix(store): resolve compile errors and LSM tree durability/consistency bugs`
  *Beseitigung von Konsistenzfehlern in MemTable und SSTable Wechselwirkungen.*
- `77ffdbe` | **google-labs-jules[bot]** | `prompter & store: add rebase-retest gate and propagate WAL recovery sync error`
  *Einbetten des Rebase-Retest Gates in xtask.*
- `c9bdada` | **google-labs-jules[bot]** | `docs(calibration): update memfuse-calibration audit report for 2026-09-10 session`
  *Dokumentationsupdates für Konform-Kalibrierung.*
- `fab2f5c` | **google-labs-jules[bot]** | `memfuse-core & memfuse-store: update audit report and fix silent IO in wal.rs`
  *Behebung stummer I/O-Fehler bei `sync_all` Aufrufen im WAL Modul.*
- `672afd0` | **google-labs-jules[bot]** | `memfuse-store: propagate errors on sync_all in recovery`
  *Erfolgreiche Weiterleitung aller I/O Fehler während der Recovery-Phase.*
- `659ee03` | **google-labs-jules[bot]** | `fix(store): propagate sync_all error in wal.rs recovery to satisfy Gate 3`
  *Erfüllung von CI Gate 3 (Keine ignorierten I/O-Aufrufe in Wiederherstellungspfaden).*
- `692382f` | **google-labs-jules[bot]** | `docs(memfuse-py): verify layer 3 pyo3 bindings audit findings for session 0b2ff57d`
  *Verifikation der Python-Schnittstelle.*
- `493a456` | **google-labs-jules[bot]** | `wal: handle sync_all error logging during backup recovery`
  *Strukturiertes Tracing bei Backup-Recovery Ausfällen.*
- `fbf8799` | **google-labs-jules[bot]** | `docs(tags): format TODO comments according to AI-TAG taxonomy and ISO-8601 rules`
  *Automatisierte Bereinigung aller freien TODO-Kommentare gemäß AI-TAG Taxonomie.*
- `c28e969` | **google-labs-jules[bot]** | `memfuse-db: sanitize sandbox bridge clippy and update audit logs`
  *Clippy-Bereinigung in Sandbox-Befehlen.*
- `cf98dea` | **google-labs-jules[bot]** | `memfuse-checkpoint: complete session audit & doc sync`
  *Abschluss des Audit-Zyklus für Checkpoints.*
- `0acd010` | **google-labs-jules[bot]** | `memfuse-index: fix clippy warning and update audit report`
  *Clippy-Fixes im Vektor-Index.*
- `6021aa6` | **google-labs-jules[bot]** | `memfuse-embed: fix Gate 3 silent I/O in wal.rs and update unwrap baseline`
  *Beseitigung stummer I/O Zugriffe in Embedder-Initialisierungen.*
- `cb78f23` | **google-labs-jules[bot]** | `memfuse-text: add unit tests for stats persistence, clone sharing, and UTF-8 slicing`
  *Sicherheits-Tests für UTF-8 Char Boundaries und Statistiken-Persistenz im Invertierten Index.*
- `9e0430f` | **google-labs-jules[bot]** | `fix(store): ensure LSM startup flush, WAL sidecar cleanup, and fix silent I/O in wal.rs`
  *Automatische WAL Sidecar Bereinigung beim Systemstart und Erzwingung des MemTable Flushes.*
- `ff3e898` | **google-labs-jules[bot]** | `fix(store): resolve LSM operational bugs and eliminate new test unwraps`
  *Beseitigung aller ungeprüften `unwrap` Aufrufe in neuen LSM-Tests.*

### 11. September 2026
- `4d8a25f` | **google-labs-jules[bot]** | `memfuse-text: verify audit pass, gate-stack, and docs sync`
  *Gate-Stack Validierung und Dokumentations-Synchronisation für `memfuse-text`.*
- `cb9c6a0` | **google-labs-jules[bot]** | `ci(bench): update HuggingFace filename parameter for LongMemEval-S download`
  *Aktualisierung der Download-Parameter für LongMemEval-S im Benchmark-Runner.*
- `6edf621` | **google-labs-jules[bot]** | `memfuse-candle: complete Tier 3 deep audit and fix inverted index compilation in memfuse-text`
  *Tier 3 Deep Audit für `memfuse-candle` und Reparatur der Kompilierungsabhängigkeiten in `memfuse-text`.*
- `919498a` | **google-labs-jules[bot]** | `memfuse-checkpoint: deep audit and tier 2 recovery verification`
  *Deep Audit & Recovery-Verifikation im Checkpoint-Crate.*
- `fa77992` | **google-labs-jules[bot]** | `docs: add doc-ref-ignore for wildcard path reference in DECISIONS.md`
  *Anpassung der Dokumentations-Referenzregeln zur Vermeidung falscher Positiver bei Wildcards.*

### 12. September 2026
- `d3e53ca` | **google-labs-jules[bot]** | `docs(audit): audit report for memfuse-calibration`
  *Erstellung und Verifizierung des Audit-Berichts `AUDIT_memfuse-calibration.md`.*
- `f3a9588` | **tfufuz1** (Co-authored-by **google-labs-jules[bot]**, **tfufuu**) | `audit: systematischer audit memfuse-text (#2243)`
  *Systematischer Tiefenaudit von `memfuse-text`: Reverifikation von BM25 Robertson-Spärck-Jones IDF, Char-Boundary Sicherheit beim Token-Slicing, sowie 100% Verifikation aller Crate-Module (`inverted.rs`, `tokenizer.rs`, `lib.rs`, `morphology.rs`).*

---

## 3. Tiefere Differenz- & Fehler-Analysen (Vorher vs. Nachher)

Um die Evolution und Behebung aller kritischen Systemfehler transparent und nachvollziehbar darzulegen, folgt eine strukturierte Gegenüberstellung nach Fachdomänen:

### A. Storage Engine & Crash Safety (LSM, WAL, SSTables)
- **Fehler / Schwachstelle**: WAL HMAC Sidecar Race & TOCTOU (`F-07`).
  *Vorher*: WAL-Integritätsschlüssel wurden nicht-atomar vor der Erstellung der WAL-Datei geprüft. Bei plötzlichem Stromausfall konnte eine teilweise geschriebene Sidecar-Datei erzeugt werden, was beim Neustart zu Korruptionsfalsch-Positiven führte.
  *Ursache*: Fehlende atomare Dateierstellung via OS-Flags (`O_EXCL` / `create_new(true)`).
  *Nachher (Lösung)*: Erstellung des WAL HMAC Sidecars mittels `create_new(true)` unter exklusivem Lock mit atomarer In-Place HMAC-Aktualisierung.
- **Fehler / Schwachstelle**: Ignorierte I/O-Fehler bei Recovery `sync_all` Aufrufen (Gate 3).
  *Vorher*: Fehler beim Erzwingen von Dateisystem-Syncs (`file.sync_all()`) während der WAL-Wiederherstellung wurden stumm mit `let _ =` verworfen.
  *Ursache*: Fehlende I/O-Fehler-Propagation im Replay-Loop.
  *Nachher (Lösung)*: Strikte Propagation aller `sync_all()` Fehler via `Result<()>` und `MemFuseError::Io`.
- **Fehler / Schwachstelle**: Read-Lock Blockaden bei LSM Flush (ADR-059).
  *Vorher*: Während `LsmStorage::flush()` wurden lesende Abfragen blockiert, da der Read-Lock auf den LSM-Tree gehalten wurde, während langsame I/O-Operationen (MemTable to SSTable Disk Write) liefen.
  *Ursache*: Monolithischer Flush-Ablauf innerhalb einer ungestaffelten Sperre.
  *Nachher (Lösung)*: 3-Phasen Lock-Free Async LSM Flush:
    1. Phase 1: In-Memory Freeze der aktiven MemTable in eine Immutables-List unter kurzer Sperre.
    2. Phase 2: Async Schreiben der SSTable-Datei auf Festplatte völlig ohne Sperre.
    3. Phase 3: Kurzer Commit-Lock zur Entfernung der Immutable MemTable und Aktualisierung des Manifests.
- **Fehler / Schwachstelle**: Flush-Counter Namens-Kollisionen (ADR-060).
  *Vorher*: Mehrere parallele `LsmStorage` Instanzen im selben Prozess wiesen identische Flush-Sequenznummern auf, was zu Überschreiben von SSTable-Dateien führte.
  *Ursache*: Statische, globale Flush-Zähler.
  *Nachher (Lösung)*: Umstellung auf instanzgebundenen `flush_counter: AtomicU64` in jedem `LsmStorage`.
- **Fehler / Schwachstelle**: Unvollständige SSTable Compaction Crash-Safety.
  *Vorher*: Wenn der Prozess während einer STCS-Compaction abstürzte, hinterließ er eine halb geschriebene SSTable-Datei, die beim nächsten Start den LSM-Tree beschädigte.
  *Ursache*: Direktes Schreiben in die Ziel-SSTable-Datei.
  *Nachher (Lösung)*: Write-Temp-Then-Rename (ADR-044): Schreiben in `.sst.tmp`, Erzwingen von `file.sync_all()`, gefolgt von atomarem `tokio::fs::rename`.

### B. Vektor- & Text-Suchindizes (HNSW, DiskANN, BM25, SIMD)
- **Fehler / Schwachstelle**: Potential UTF-8 Char Boundary Panic im Text Slicing.
  *Vorher*: Beim Zuschneiden von Strings an festen Byte-Indizes in `memfuse-text` konnte ein Slicing mitten in einem Multibyte-UTF-8-Zeichen auftreten und eine Panik auslösen.
  *Ursache*: Direkter Zugriff auf `&str[..len]` ohne Boundary-Prüfung.
  *Nachher (Lösung)*: Absicherung via `floor_char_boundary` / `char_indices` Prüfungen, die sicherstellen, dass Slices strikt an UTF-8 Zeichengrenzen ausgerichtet sind.
- **Fehler / Schwachstelle**: SIMD Nightly-Dependence & Instabilität (ADR-047).
  *Vorher*: Distanzberechnungen nutzten `std::simd` (Nightly Rust Compiler requirement), was zu Build-Inkompatibilitäten und Crashs führte, wenn die CPU ein Feature nicht unterstützte.
  *Ursache*: Implizite Annahme von AVX2/AVX-512 Support ohne Laufzeit-Erkennung.
  *Nachher (Lösung)*: Umstellung auf Stable Rust `std::arch` Intrinsics mit 100% Abdeckung durch Laufzeit-Feature-Erkennung (`is_x86_feature_detected!`, `is_aarch64_feature_detected!`) und geschützten Skalar-Fallbacks. Kosinus-Distanzen werden strikt auf `[0.0, 2.0]` geclampt (`dist.clamp(0.0, 2.0)`).
- **Fehler / Schwachstelle**: Falsche BM25 IDF Formel im Invertierten Index.
  *Vorher*: Die BM25 Implementierung nutzte ein klassisches Unsmoothed Log-IDF, das bei Termen, die in mehr als der Hälfte der Dokumente vorkamen, negative Scores erzeugte.
  *Ursache*: Mathematisch ungeeignete Formel für begrenzte Sammlungen.
  *Nachher (Lösung)*: Umstellung auf Robertson-Spärck-Jones smoothed log-IDF $\ln\left(1 + \frac{N - df + 0.5}{df + 0.5}\right)$, das stets nicht-negativ bleibt, ergänzt durch strikte Hyperparameter-Validierung ($k_1 \ge 0.0$, $0.0 \le b \le 1.0$).
- **Fehler / Schwachstelle**: DiskANN Datenverlust bei abruptem Absturz.
  *Vorher*: Neue Vektoren in DiskANN verblieben vor dem Flushing im Hauptspeicher ohne WAL-Absicherung; ein Crash führte zu Datenverlust.
  *Ursache*: Fehlen einer transaktionalen Pufferung.
  *Nachher (Lösung)*: DiskANN WAL-gepufferter Pending Buffer und inkrementelle Delta-Persistierung (`persist_delta`), die ungeflushte Daten aus dem WAL rekonstruiert.

### C. Wissensgraph & Kognitive Mechanismen
- **Fehler / Schwachstelle**: Misleitende biologische Metapher-Begriffe im Code.
  *Vorher*: Kernkomponenten der Datenbank verwendeten Begriffe wie `sleep-cycle`, `thermostat` und `reaper`.
  *Ursache*: Frühe Entwurfs-Metaphern, die zu Unklarheiten im API-Design führten.
  *Nachher (Lösung)*: Refactoring in `memfuse-db` (`#2d37e40`): Ersetzung aller biologischen Metaphern durch präzise technische Bezeichnungen (`ConsolidationScheduler`, `PidController`, `ExpiryEngine`).
- **Fehler / Schwachstelle**: SessionBranchTree Lock-Ordering Deadlock (#1539).
  *Vorher*: Bei simultanen Branch-Switches und Vertiefungs-Operationen im Session-DAG kam es zu zyklischen Deadlocks zwischen `AppState.sessions` (`RwLock`) und `SessionBranchTree` internal Locks.
  *Ursache*: Inkonsistente Sperr-Reihenfolge in async Kontexten.
  *Nachher (Lösung)*: Einführung eines Newtype Guards, der die Sperrhierarchie strikt durchsetzt (1. `AppState.sessions` -> 2. `SessionBranchTree`), sodass Locks vor `.await` Punkten vollständig freigegeben werden.
- **Fehler / Schwachstelle**: Verwaiste Kanten bei Gedächtnis-Verdrängung (A-MEM Zettelkasten).
  *Vorher*: Wenn ein veraltetes Dokument durch ein neues ersetzt wurde (`LinkRelation::Supersedes`), blieben die zugehörigen Entitäts-Kanten im CSR-Graph aktiv und verfälschten spätere GraphRAG-Abfragen.
  *Ursache*: Fehlende kaskadierende Invalidierung im Graph-Modul.
  *Nachher (Lösung)*: Kaskadierende Kanten-Invalidierung (`cascade.rs`, `INV-GRAPH-PROV-1`): Das System speichert `source_doc_id` in `Edge` und pflegt einen `doc_to_edges` Index (`DocId -> Set<(EntityId, EntityId)>`). Bei `Supersedes` wird `cascade_invalidate_edges_for_superseded_doc()` aufgerufen und invalidierte Kanten transaktional mit WAL-Bindung aus dem aktiven Traversierungsgraphen entfernt.

### D. Konkurrenz, Transaktionen & Sperrhierarchien
- **Fehler / Schwachstelle**: Unvollständiger Transaktions-Commit über verteilte Indizes.
  *Vorher*: Ein Fehlschlag beim Schreiben in den CSR-Graph hinterließ bereits geschriebene Vektoren im HNSW-Index und Texte im BM25-Index, was die Cross-Signal-Konsistency zerstörte.
  *Ursache*: Fehlen einer verteilten Transaktionskontrolle.
  *Nachher (Lösung)*: Full 4-Index 2-Phase Commit (2PC):
    1. Phase 1 (Prepare): Alle 4 Sub-Engines (HNSW, BM25, CSR-Graph, Metadaten) validieren Eingaben und schreiben Änderungen in Shard-basierte Staging-Buffer (`TxBuffer`).
    2. Phase 2 (Commit/Rollback): Bei Erfolg aller 4 Vorbereitungen wird ein atomarer WAL Commit ausgeführt; bei einem Teilfehler wird ein kompensierendes Rollback über alle vorbereiteten Indizes getriggert.
- **Fehler / Schwachstelle**: TOCTOU Race Condition bei Key-Locks (`KvKeyLocks`).
  *Vorher*: Schlüssel-Sperren wurden über ein globales Modul synchronisiert, was zu Race Conditions zwischen voneinander unabhängigen Datenbank-Sammlungen führte.
  *Ursache*: Ungenügende Mandanten- und Instanz-Isolierung.
  *Nachher (Lösung)*: `KvKeyLocks` wurden instanzgebunden direkt an die `Collection` gebunden. Mutation-Operationen finden innerhalb der `insert_lock` Mutex-Sperre statt, was TOCTOU-Kollisionen vollständig ausschließt.

### E. Sicherheit, FFI, Mandantenfähigkeit & Compliance
- **Fehler / Schwachstelle**: Umgehen von Mandanten-Isolierung via `TenantId::new()`.
  *Vorher*: Entwickler konnten `TenantId::new()` ohne Validierung aufrufen, was `INV-TENANT-1` verletzen konnte.
  *Ursache*: Fehlende Deprecation und Durchsetzung von `TenantId::try_new()`.
  *Nachher (Lösung)*: `TenantId::new()` als `#[deprecated]` markiert und durch `TenantId::try_new()` sowie `TenantId::SYSTEM` ersetzt. Strikte Crate-Aliasierung von `memfuse-crypto` zu `memfuse-security`.
- **Fehler / Schwachstelle**: Fehlen eines kryptographischen Löschnachweises (DSGVO Art. 17).
  *Vorher*: Das Löschen von Dokumenten konnte nicht nachweisbar belegt werden.
  *Ursache*: Einfaches Setzen von Tombstones ohne kryptographische Signatur.
  *Nachher (Lösung)*: `DeletionProof`: Nach dem physischen Löschen generiert das System einen kryptographischen Löschnachweis mit HMAC-Signatur über `DocId`, `TenantId`, Timestamp und Merkle-Root der gelöschten Blöcke.
- **Fehler / Schwachstelle**: CPython Process Crash bei Rust Panics über FFI.
  *Vorher*: Ein Panic in rechenintensiven Rust-Funktionen führte zum sofortigen Absturz der gesamten Python-Anwendung.
  *Ursache*: Unverfangene Rust-Panics über PyO3 FFI-Grenzschichten.
  *Nachher (Lösung)*: Kapselung aller FFI-Aufrufe in `run_blocking_ffi` mit `py.allow_threads()` (Freigabe des Python GIL) und `std::panic::catch_unwind`, das Rust Panics abfängt und sauber in Python `PyRuntimeError` Exceptions konvertiert.
- **Fehler / Schwachstelle**: MCP Stdio RPC Buffer Overflow DoS Vector.
  *Vorher*: Der MCP Stdio JSON-RPC Server erlaubte Puffergrößen bis zu 16 MB pro Nachricht, was zu Speichererschöpfung (OOM) bei der Nachrichtenverarbeitung führte.
  *Ursache*: Zu hoch angesetztes Pufferlimit (`MAX_RPC_BYTES = 16MB`).
  *Nachher (Lösung)*: Reduktion von `MAX_RPC_BYTES` auf 4 MB, Pufferdeckelung in `read_line_bounded` und Integration von Prompt Injection Guards für eingebettete Werkzeuge.

### F. Agenten, Kalibrierung & Adaptive Systeme
- **Fehler / Schwachstelle**: TokenBudget Concurrency Read-Modify-Write (RMW) Race (#1239).
  *Vorher*: Parallele Schritte in Agenten-Workflows konnten das zugewiesene Tokenbudget überschreiten, da Budget-Prüfung und Budget-Abzug nicht atomar waren.
  *Ursache*: Getrennte Lese- und Schreibzugriffe auf `TokenBudget` ohne atomaren Schutz.
  *Nachher (Lösung)*: Atomare RMW-Operationen auf `TokenBudget` mit APM-4 Pre-Execution Validierung vor `tool.execute()`.
- **Fehler / Schwachstelle**: Starre Signal-Gewichtung beim Hybrid Retrieval.
  *Vorher*: Die Gewichtung der RRF-Signale (Vektor, Text, Graph) war statisch und konnte sich nicht an wechselnde Dokumentenstrukturen anpassen.
  *Ursache*: Hartcodierte Gewichte.
  *Nachher (Lösung)*: Replicator Dynamics: Dynamische Signal-Gewichtung, die RRF-Gewichte auf Basis zeitdiskreter Replikatordynamik basierend auf Verdrängung und Retrieval-Latenzen anpasst.
- **Fehler / Schwachstelle**: Nicht-kalibrierte Relevanz-Scores bei SLM Prompt-Routing.
  *Vorher*: Raw Cross-Encoder Scores führten zu fehlerhaftem Routing, da Vertrauenswerte nicht mit der tatsächlichen Wahrscheinlichkeit übereinstimmten.
  *Ursache*: Fehlen einer Post-Hoc Kalibrierung.
  *Nachher (Lösung)*: `memfuse-calibration`: Integration von Isotonischer Regression und Platt-Scaling zur konformen Kalibrierung von Relevanz-Scores.

---

## 4. Subsystem- & Crate-Entwicklung (Layer 0 bis Layer 4)

Das Repository umfasst **18 aktiv verwaltete Workspace Crates**, aufgeteilt in 5 Architektur-Schichten:

```
┌─────────────────────────────────────────────────────────────────┐
│ Layer 4: Desktop App, MCP Server & Kalibrierung                 │
│  - memfuse-tauri (Tauri Desktop App Shell)                      │
│  - memfuse-mcp (JSON-RPC 2.0 MCP Server & Stdio Sandbox)        │
│  - memfuse-router (SLM Context Routing Engine)                  │
│  - memfuse-bench (Reproduzierte Benchmark-Suite)                │
├─────────────────────────────────────────────────────────────────┤
│ Layer 3: Client Interfaces & RAG Augmentation                   │
│  - memfuse-db (4-Signal Fusion, MultiStep Engine, Compactor)    │
│  - memfuse-py (PyO3 Python FFI Bindings)                        │
│  - memfuse-ollama (Lokales LLM / Embedding Backend)             │
│  - memfuse-agent (Persistent Agent Workflow Engine)             │
│  - memfuse-embed (In-Process ONNX Reranking & Embeddings)       │
├─────────────────────────────────────────────────────────────────┤
│ Layer 2: Storage & Spezialisierte Indizes                       │
│  - memfuse-store (LSM-Tree, WAL V3, MemTable Sharding)         │
│  - memfuse-index (HNSW, DiskANN, Quantisierung, SIMD)           │
├─────────────────────────────────────────────────────────────────┤
│ Layer 1: Spezifische In-Memory & Hilfs-Crates                   │
│  - memfuse-text (BM25, Invertierter Index, Morphologie)         │
│  - memfuse-crypto / memfuse-security (AES-256-GCM-SIV, WAL)     │
│  - memfuse-graph (CSR-Graph, PathRAG, Bi-temporal Axes)        │
│  - memfuse-checkpoint (MVCC Snapshot-Pinning, CheckpointGuard) │
│  - memfuse-kv-bridge (Tenant-Isolated KV-Segment Store)         │
│  - memfuse-calibration (Isotonic & Platt Conformal Calibration) │
│  - memfuse-candle (Native Candle GGUF ML Inference)             │
├─────────────────────────────────────────────────────────────────┤
│ Layer 0: Core Abstractions & Data Types                         │
│  - memfuse-core (MemFuseError, Domain Types, TenantId, Traits)  │
└─────────────────────────────────────────────────────────────────┘
```

### Detaillierte Crate-Rollen (18 Crates):
1. **`memfuse-core`** (Layer 0): Kanonische Domain-Typen (`TenantId`, `DocId`, `TxId`), Unified `MemFuseError`, Trait-Definitionen und `TxBuffer`.
2. **`memfuse-calibration`** (Layer 1): Isotonische und Platt Conformal Calibration für Konfidenz-Scoring.
3. **`memfuse-candle`** (Layer 1): Native Candle GGUF ML-Inferenz-Backend für rahmenwerksfreie Embeddings.
4. **`memfuse-checkpoint`** (Layer 1): MVCC Snapshot-Pinning und `CheckpointGuard` RAII Rollbacks.
5. **`memfuse-crypto` / `memfuse-security`** (Layer 1): AES-256-GCM-SIV Blockverschlüsselung, HKDF Key Derivation, WAL HMAC Chaining und `DeletionProof`.
6. **`memfuse-graph`** (Layer 1): CSR-Wissensgraph, bi-temporale Zeitachsen, Session-DAG, PathRAG Engine, F-06 Perkolationsmonitor und kaskadierende Kanteninvalidierung (`cascade.rs`).
7. **`memfuse-kv-bridge`** (Layer 1): Hochperformante, mandantenisolierte KV-Cache-Schicht mit `ZeroizeOnDrop` Garantien.
8. **`memfuse-text`** (Layer 1): Invertierter Index, BM25 Scorer (Robertson-Spärck-Jones) und deutsche Morphologie.
9. **`memfuse-embed`** (Layer 2): In-process ONNX Session Pool für Cross-Encoder Reranking mit Chaos Engineering Schutz.
10. **`memfuse-index`** (Layer 2): HNSW Vektorindex, Streaming DiskANN mit Beam Search, SQ8 Quantisierung und `std::arch` SIMD Intrinsics.
11. **`memfuse-ollama`** (Layer 2): HTTP-Client für lokale Ollama LLMs/Embeddings mit Batch-Streaming und Anthropic Contextual Retrieval.
12. **`memfuse-store`** (Layer 2): LSM-Tree mit MemTable-Sharding, 3-Phasen Lock-Free Async Flush, WAL V3 und SSTable Compaction.
13. **`memfuse-db`** (Layer 3): Orchestrator Facade für 4-Signal Hybrid Retrieval (RRF), Full 2PC Transactions, PID Latency Regulation und Replicator Dynamics.
14. **`memfuse-agent`** (Layer 3): Workflow-Engine mit Event-Loops, State Graph Walkers, Dead-Letter-Queues und TokenBudget RMW Schutz.
15. **`memfuse-py`** (Layer 3): PyO3 Python FFI Bindings mit automatischer GIL-Freigabe und Panic Catching.
16. **`memfuse-bench`** (Layer 4): Reproduzierbare Benchmark-Suite für Retrieval-Genauigkeit, Durchsatz und Latenz-Perzentile (inkl. LongMemEval & LoCoMo Datensätze).
17. **`memfuse-router`** (Layer 4): SLM Context Routing Engine für adaptives Prompt-Routing.
18. **`memfuse-tauri`** (Layer 4): Desktop App Shell mit HTML Sanitizing (XSS Protection) und abgesicherter IPC Ingestion.

---

## 5. Governance & Qualitäts-Sicherung

Die Projekt-Historie zeichnet sich durch ein streng durchgesetztes Governance-System aus:

1. **Architecture Decision Records (ADRs)**: Strikte Einhaltung aller Vorgaben (z.B. ADR-010 Stdio MCP, ADR-012/043 MVCC Isolation, ADR-028 Error DTOs, ADR-044 Write-Temp-Then-Rename, ADR-047 SIMD Intrinsics, ADR-059 Non-Blocking Async LSM Flush, ADR-060 Instance-Scoped Flush Counter, ADR-068 KV-Encryption, ADR-073 Contradiction Prevention).
2. **Inline Code Tags & Review Passes**: Verwendung von `ANCHOR[...]`, `AI-TAG[...]` und `REVIEW-PASS[...]` Annotationen mit ISO-8601 Zeitstempeln (`TS:2026-09-12T...`) und Session-Hashes.
3. **Automatisierte CI Enforcement Gates**:
   - `cargo xtask check-consistency`: Überprüft exakt 18 Workspace-Crates, `AGENTS.md` Abdeckung, README-Auszüge und ADR-Eindeutigkeit.
   - `cargo xtask sync-docs`: Verhindert Drift zwischen Quellcode-Annotationen und Dokumentationsdateien (`WORKING_STATE.md`, `ARCHITECTURE.md`, `CHANGELOG.md`, `SOURCE_OF_TRUTH.md`).
   - `context-gates.yml`: Verhindert ungelöste `CRITICAL` Code Smells, phantom temporäre Dateien (Gate 12b) und prüft die Gültigkeit von Anchor-Tags.

---

## 6. Statistische Kennzahlen

- **Aktive Workspace Crates**: 18 Crates (Layer 0 bis Layer 4)
- **Commits insgesamt**: >320 Merges und Direkt-Commits
- **Verteilte Autoren**: `google-labs-jules[bot]`, `tfufuz1`, `tfufuu`
- **Programmiersprache**: 100% Rust (mit Tauri UI HTML/JS Frontend & PyO3 Python-Interface)
- **Sicherheit & Zero-Panic Policy**: Volle Beseitigung aller unkontrollierten `.unwrap()` Aufrufe in Produktivpfaden (abgesichert via `// unwrap allowed` mit nachgewiesenen Invarianten und `.unwrap-baseline.json`).

---

*Ende der Projekthistorie.*
