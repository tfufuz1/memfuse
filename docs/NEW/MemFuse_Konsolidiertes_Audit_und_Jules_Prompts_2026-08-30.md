# MemFuse — Konsolidiertes Audit & Google-Jules-Implementierungsprompts

> **Verifikationsdatum:** 2026-08-30
> **Repo-Commit:** `a399265d` (main, frischer Clone von `github.com/tfufuz1/memfuse`)
> **Methode:** Zeile-für-Zeile-Code-Verifikation aller 15 zugelieferten Audit-/Prompt-Dokumente gegen den tatsächlichen Quellcode (grep, view, gezielte Struktur-Analyse). Keine Ausführung von `cargo build`/`cargo test` in dieser Umgebung — alle Befunde sind statisch am Quelltext verifiziert.
> **Zugelieferte Dokumente:** `MEMFUSE_INTERFACE_SPECIFICATION.md`, `MEMFUSE_SOURCE_OF_TRUTH_STRATEGY.md`, `Memfuse-Prompter-v3.html`, `memfuse_umsetzungsprompts(_1_).md`, `memfuse_jules_implementierungsprompts_2026-08-29.md`, `MemFuse_Vollaudit_und_Jules_Prompts_2026-08-28.md`, `memfuse_audit_jules_prompts.md`, `MemFuse_Audit_und_Jules_Implementierungsprompts.md`, `memfuse_codesmells_detailed.md`, `memfuse_v2_optimierungsspezifikation.md`, `memfuse-jules-prompts.md`, `memfuse-jules-prompts-v2.md`, `memfuse-verification-and-phase3-prompts.md`, `memfuse-sme-rag-strategie.md`, `memfuse-fix-plan.md`

---

## 0. Executive Summary

Der aktuelle Stand von MemFuse übertrifft den in den meisten Zulieferdokumenten beschriebenen Ist-Zustand deutlich. Die Git-Historie zeigt eine sehr aktive, iterative Härtungs-Serie (`Harden memfuse-*`-Commits) sowie mehrere vollständig durchgeführte Google-Jules-Sitzungen. Von den in den älteren Audits (Fix-Plan, v2-Prompt-Serie, Audit-Jules-Prompts) beschriebenen kritischen Findings (C-1 bis C-3) ist **keines mehr offen**. Von den 12 in der v2-Serie beschriebenen Feature-Prompts sind **alle 12 vollständig umgesetzt**. Von den Phase-3-Prompts (13–20, UI-Vervollständigung) sind **alle 8 vollständig umgesetzt**. Von den 9 Prompts der neuesten Session (P-01 bis P-09, 2026-08-29) sind **6 vollständig, 2 teilweise und 1 gar nicht** umgesetzt.

Die verbleibenden offenen Punkte lassen sich in drei Kategorien einteilen:

1. **Kleine, konkrete Restarbeiten** aus bereits größtenteils erledigten Prompt-Sessions (z. B. fehlender ADR-Eintrag, fehlendes Config-Feld, tote Error-Variante) — niedriger bis mittlerer Aufwand, hohe Priorität wegen Konsistenz.
2. **Echte, bisher unangetastete Performance-/Hygiene-Befunde** aus dem tiefen Architektur-Audit (H-1, H-4 bis H-10, M-1 bis M-6) — mittlerer Aufwand, mittlere Priorität.
3. **Große, komplett unimplementierte Forschungs-Roadmap-Punkte** (`memfuse-quant`, `memfuse-kv`, `ProvenanceRecord`, `CausalEdge`, Vamana-Index, io_uring) aus den beiden Zukunfts-Spezifikationsdokumenten — hoher Aufwand, niedrige/mittlere Priorität, da es sich um Vision statt Bugfix handelt.

Alle Prompts in Abschnitt 3 sind gegen den **tatsächlichen, aktuellen Code** formuliert — nicht gegen die (teils veralteten) Codebeispiele der Ursprungsdokumente.

---

## 1. Vollständige Status-Matrix — Erledigt vs. Offen

### 1.1 Aus `memfuse-fix-plan.md`

| # | Finding | Status | Verifikationsbefund |
|---|---|---|---|
| 1.1 | MCP-Server Nullvektor-Bug | ✅ Erledigt | `memfuse-ollama` als eigener Crate extrahiert, `McpServer.embedder: Arc<dyn TextEmbeddingEngine>`, `handle_insert` ruft `self.embedder.embed(...)` echt auf |
| 1.2 | `namespace.rs` reparieren oder entfernen | 🟡 Teilweise | Datei entfernt, aber `MemFuseError::NamespaceViolation` bleibt als **toter Code** in `error.rs`, `error_dto.rs`, `memfuse-py`, `memfuse-mcp/protocol.rs` — wird nirgends geworfen |
| 1.3 | `FusionWeights` an Fusion anschließen | ✅ Erledigt | `weighted_reciprocal_rank_fusion()` implementiert, bis `hybrid_search_with_weights()` und Python-Bindings (`hybrid_search`, `hybrid_search_fb`) durchgereicht |
| 2.1 | Ingestion parallelisieren | ✅ Erledigt | `buffer_unordered(EMBED_CONCURRENCY)` in `pipeline.rs` |
| 2.2 | `memfuse-embed` spawn_blocking | ✅ Erledigt | `embed_async()` nutzt `tokio::task::spawn_blocking` |
| 2.3 | MemTable Sharding | ✅ Erledigt | 16 Shards, BLAKE3-basierter `shard_for()`, analog zu `TxBuffer`-Muster |
| 2.4 | `flush()` Lesbarkeit während Flush | ✅ Erledigt | Write-Lock wird nur kurz gehalten, teures SSTable-Schreiben läuft außerhalb des Locks (`drop(state)` vor `builder.finish()`) |
| 2.5 | ARM/NEON-Pfad | ✅ Erledigt | `cosine_distance_neon`, `euclidean_distance_neon`, `dot_product_neon` mit `is_aarch64_feature_detected!` |
| 2.6 | SSTable Prefix-Scan Key-Range-Vorabcheck | ✅ Erledigt | `scan_prefix_at()` prüft `first_key`/`last_key`-Range inkl. korrektem Präfix-Ende-Vergleich |
| 2.7 | Token-Schätzung Deutsch kalibrieren | ❌ **Nicht erledigt** | `estimate_tokens()` wurde generisch verbessert (BPE-Approximation, CJK, Code-Blöcke), aber **keine explizite Deutsch-/Komposita-Kalibrierung** |
| 3.x | `HnswConnectivityDegraded` toter Code | ✅ Erledigt | Wird jetzt aktiv in `hnsw.rs` geworfen (nicht mehr nur deklariert) |
| 3.x | u8-Overflow `compute_u8` | ✅ Erledigt | Akkumulation korrekt in `u64` mit `.min(u32::MAX as u64)`-Sättigung |
| 3.x | `memfuse-checkpoint` FROZEN/SAOS-Kommentar | ✅ Erledigt | Kommentar korrekt auf aktive Nutzung umgeschrieben |
| 3.x | ADR-008 Ollama-Ergänzung | ✅ Erledigt | `DECISIONS.md` Zeile 78 dokumentiert Ollama-Wechsel |
| 3.x | MCP Tool-Beschreibung "vector+BM25+metadata" | ✅ Erledigt | Korrigiert zu "vector + BM25 + graph" |
| 3.x | `GermanCompoundSplitter`-Doku (lowercase-Hinweis) | ✅ Erledigt | Doctest zeigt expliziten `normalize_umlauts()`-Vorverarbeitungsschritt |
| 3.x | `delete_prefix`-Default durch Batch-Tombstone ersetzen | 🟡 Teilweise | `memfuse-store::LsmStorage::delete_prefix()` nutzt bereits `stage_many()` (echter Batch); der **Default-Trait-Impl in `traits.rs`** bleibt weiterhin die naive Pro-Key-Schleife |

### 1.2 Aus `memfuse-jules-prompts-v2.md` (12 Prompts) und `memfuse-verification-and-phase3-prompts.md` (Prompts 13–20)

| # | Prompt | Status |
|---|---|---|
| 1–12 | Sofort-Fixes, `delete_prefix`/`scan_prefix` im Trait, Graph-Persistenz, Graph in `hybrid_search()`, FIND-STO-001/FIND-DB-002, Tauri-Grundgerüst, Ingestion-Pipeline, Deutsche Morphologie, Ollama-Bridge, Tauri-Commands, MCP-Server, Dokumentation | ✅ **Alle 12 vollständig erledigt** (unabhängig verifiziert) |
| 13 | `lru`-CVE (RUSTSEC-2026-0002) schließen | ✅ Erledigt — `lru = "0.16.3"` in `memfuse-store/Cargo.toml` |
| 14 | Tauri-UI: Datenbank & Collections | ✅ Erledigt — Sidebar mit `open_database`, `list_collections`, `create_collection`, `drop_collection` vollständig verdrahtet |
| 15 | Tauri-UI: Dokumenten-Import mit Fortschrittsanzeige | ✅ Erledigt — `ingest_file`/`ingest_folder` im Frontend aufgerufen |
| 16 | Tauri-UI: Suche & Quellenanzeige im Chat | ✅ Erledigt — `hybrid_search` separat aufrufbar, Quellen im Chat sichtbar |
| 17 | E2E-Integrationstest Ingest→Search→Chat | ✅ Erledigt — `crates/memfuse-tauri/tests/e2e_test.rs` |
| 18 | Graph-Entity-Extraktion bei Ingestion | ✅ Erledigt — `SimpleEntityExtractor` + `graph.add_entity()` in `pipeline.rs` |
| 19 | Onboarding-Assistent | ✅ Erledigt — `#onboarding-overlay` mit 3-Schritt-Flow in `index.html` |
| 20 | Installer-Konfiguration (Win/macOS/Linux) | ✅ Erledigt — vollständiges `tauri.conf.json`-Bundle mit NSIS, deutschsprachigem Installer |

Auch `memmap2` (RUSTSEC — separates CVE) wurde bereits auf `0.9.11` gepatcht.

### 1.3 Aus `memfuse_jules_implementierungsprompts_2026-08-29.md` (P-01 bis P-09, neueste Session)

| # | Prompt | Status | Detail |
|---|---|---|---|
| P-01 | CRITICAL Tags: `AGT-CKPT-f3a1b2c4` + `AGT-STORE-003` | ✅ Erledigt | `manifest_fault_injection.rs` und `wal_key_lifecycle.rs` mit expliziten Crash-Simulationstests vorhanden |
| P-02 | TxBuffer Bounded Capacity + Snapshot-Hardening | ✅ Erledigt | `max_ops_per_tx` in `TxBufferConfig`, `stage()`/`stage_many()` setzen Grenze durch, `snapshot.rs` `.expect()` nur noch in Proptest-Blöcken, `allocate_tx()`/`next_tx()` geben `Result` zurück, FlatBuffers-Fallback dokumentiert |
| P-03 | Kognitive Gedächtnistypen-System | 🟡 Teilweise | `MemoryType`-Enum (Episodic/Semantic/Procedural/Working), `insert_typed()`, `extract_memory_type()`, Tests — alles vorhanden. **ADR-028 fehlt in `DECISIONS.md`** (Nummer bereits von anderem Thema belegt) |
| P-04 | Memory Lifecycle: Decay + TTL + LLM-Importance | ✅ Erledigt | `trigger_reaper()` mit TxId-basiertem Decay-Sweep, `MemoryLifecycleManager`-Trait, `evaluate_importance_with_llm()`, alle 4 geforderten Tests vorhanden |
| P-05 | Bi-temporaler Graph `traverse_at_time` | ✅ Erledigt | Vollständig implementiert inkl. `is_edge_visible()`, Tombstone-Handling, keine `#[ignore]`-Tests mehr |
| P-06 | WP14: `scan_prefix_at` + `search_at` | ✅ Erledigt | Beide implementiert (HNSW via `SequenceLog`/`is_visible()`), `SOURCE_OF_TRUTH.md` auf 🟢 aktualisiert |
| P-07 | PPR-Hardening + Community-Proptest + God-Object-Refactoring | 🟡 Teilweise | Refactoring vollständig (`collection/` in 6 Submodule, `mod.rs` von ~2900 auf 456 LOC). PPR-Proptest vorhanden (`prop_ppr_rank_mass_conservation`, funktional äquivalent zur Vorlage). **`PprConfig.warn_on_non_convergence` fehlt. Sink-Node-Handling nicht auffindbar. Community-Detection hat 0 Proptests** (nur konventionelle Unit-Tests inkl. Determinismus-Test) |
| P-08 | Zero-Copy Clone-Reduktion (L3→L5) | ❌ **Nicht erledigt** | `lsm.rs` hat weiterhin exakt 32 `.clone()`-Aufrufe (unverändert), `scan_prefix()` gibt weiterhin `Vec<(Vec<u8>, Vec<u8>)>` statt `Bytes`, kein Clone-Reduktions-Benchmark vorhanden |
| P-09 | FFI-Boundary Hardening (Tauri/PyO3/MCP) | ✅ Erledigt | `main.rs` ohne `.expect()`, alle Tauri-Commands nutzen `MemFuseErrorDto`, PyO3 mappt auf semantische Exception-Typen, MCP nutzt `MemFuseErrorDto` im `error.data`, `AGENTS.md` unsafe-Scope aktualisiert |

### 1.4 Aus `memfuse_audit_jules_prompts.md` (Tiefenaudit C/H/M-Level)

| ID | Befund | Status |
|---|---|---|
| C-1 | Unvollständiges 2PC: Text-/Graph-Index fehlten im Commit | ✅ Erledigt — `DbTransaction::commit()` hat vollständige 4-Index-Sequenz mit Rollback-Kompensation pro Fehlerpunkt |
| C-2 | WAL-HMAC ohne `tx_id` | ✅ Erledigt — `compute_checksum_v3()` bindet `tx_id` ein, sauberer V2→V3-Migrationspfad |
| C-3 | `panic!` bei TxId-Exhaustion | ✅ Erledigt — `next_tx()`/`allocate_tx()` geben `Result` zurück |
| H-1 | Blake3 im MemTable-Hot-Path (Performance) | ❌ **Nicht erledigt** — `shard_for()` nutzt weiterhin `blake3::hash()` |
| H-2 | 2PC-Rollback kompensiert Text/Graph nicht | ✅ Erledigt (Teil von C-1-Fix) |
| H-3 | Redundante API `next_tx()`/`allocate_tx()` | ❌ **Nicht erledigt** — `next_tx()` bleibt `pub`, wird weiterhin extern (`memfuse-checkpoint`) aufgerufen |
| H-4 | `scan_prefix_at`-Default gibt `PolicyViolation` statt `CapabilityUnsupported` | ❌ **Nicht erledigt** — Inkonsistenz besteht weiterhin gegenüber allen anderen `*_at`-Defaults |
| H-5 | NaN-Validierung im Distance-Hot-Path (Performance) | ❌ **Nicht erledigt** — `compute_distance()` prüft weiterhin bei jedem Aufruf, obwohl HNSW bereits beim Insert prüft (doppelte Arbeit) |
| H-6 | Dual Entry-Point HNSW (`entry_point` + `ram_entry_point`) | ❌ **Nicht erledigt** — beide Felder bestehen weiterhin nebeneinander |
| H-7 | TxBuffer ohne Kapazitäts-Enforcement | ✅ Erledigt (Teil von P-02) |
| H-8 | `WorkflowState::graph_hash` als `String` statt `[u8; 32]` | ❌ **Nicht erledigt** |
| H-9 | CSR `compact()` O(N) Full-Rebuild pro Delta-Flush | ❌ **Nicht erledigt** — Schleife iteriert weiterhin über alle Knoten |
| H-10 | InvertedIndex Default-Namespace-Kollision | ❌ **Nicht erledigt** — beide Zweige produzieren weiterhin identischen Prefix für `"default"` |
| M-1 | `rebuild_threshold`-Benennung invertiert/missverständlich | ❌ **Nicht erledigt** |
| M-2 | `unwrap_or_else(\|\| panic!(...))` in Compaction | ❌ **Nicht erledigt** — Zeile 917 in `compaction.rs` |
| M-3 | BM25-IDF-Floor `1e-6` statt `0.0` | ❌ **Nicht erledigt** |
| M-4 | Shadow-Entities bei `get_or_create_index` | 🟡 Teilweise — Grundproblem besteht (Kommentar bestätigt es explizit), aber Traversal-Code ist bereits durchgängig robust gegen `None`-Entities (`.is_some_and(...)`-Guards an allen Fundstellen) |
| M-5 | ScalarQuantizer ohne automatischen Recalibration-Trigger | ❌ **Nicht erledigt** — nur manuelle Config-Option und Log-Hinweis |
| M-6 | `MemTable::get()` nutzt `versions.last()` statt `max_by_key` | ❌ **Nicht erledigt** — `get_at_seq()` (Snapshot-Variante) ist bereits korrekt, `get()` (Hot-Path) nicht |

### 1.5 Aus `memfuse_codesmells_detailed.md`

| # | Code-Smell | Status |
|---|---|---|
| 1 | Panic-Kette in Reaper (`.unwrap()`) | ✅ Erledigt — `start_orphan_reaper()` nimmt fertig konstruierten `Arc<HnswIndex>` als Parameter statt intern zu unwrappen. Globale Produktions-Unwrap-Zahl auf ~31 gesunken (fast alle in Benchmarks/generiertem FlatBuffers-Code, kein handschriftlicher Kern-Code betroffen) |
| 2 | String-basierte Error-Propagation an FFI-Grenzen | ✅ Erledigt (deckungsgleich mit P-09) |
| 3 | Stille Default-Implementierungen Snapshot-Isolation | ✅ Erledigt (deckungsgleich mit P-06, `capability_coverage`-Testmodul als Integrationstest-Absicherung) |
| 4 | Inkonsistente Async-Trait-Patterns (`SandboxBridge`) | ✅ Erledigt — `SandboxBridge` nutzt jetzt `#[async_trait::async_trait]` |

### 1.6 Aus `memfuse-sme-rag-strategie.md`

| Behauptung im Dokument | Code-Realität heute |
|---|---|
| "Python-Bindings: 0 Tests, nicht im Build" | ❌ **Überholt** — `memfuse-py` ist Workspace-Member, hat umfangreiche Pytest-Suite (`test_bindings.py`, `test_mcp_real.py`, `test_gil_concurrency.py`, `test_recovery.py`, `test_errors.py`) |
| "Graph verliert Daten bei Neustart" (FIND-GRA-001) | ❌ **Überholt** — Graph-Persistenz vollständig implementiert (siehe v2-Prompt 3) |
| "2 aktive CVEs" (`memmap2`, `lru`) | ❌ **Überholt** — beide gepatcht |
| Enterprise-RAG-Features (Ingestor, LangChain-Integration, Admin-Dashboard, Multi-Tenancy) | Teilweise vorhanden (Ingestion-Pipeline, Tauri-UI als Admin-Dashboard-Äquivalent), aber **kein `pip install memfuse`-fertiges Paket, keine LangChain-Integration, keine formale Abteilungs-Isolation/Namespace-API** — bleibt strategische Zukunftsarbeit, nicht Bugfix |

### 1.7 Aus `MEMFUSE_INTERFACE_SPECIFICATION.md`, `MEMFUSE_SOURCE_OF_TRUTH_STRATEGY.md`, `memfuse_v2_optimierungsspezifikation.md`

Diese drei Dokumente beschreiben eine **kohärente, aufeinander aufbauende Forschungs-Roadmap** (nicht Bugfixes). Verifikation ergab: **Keiner der zentralen neuen Bausteine existiert im Code.**

| Vorgeschlagener Baustein | Ziel-Crate | Status |
|---|---|---|
| `ProvenanceRecord` (abfragbares Herkunfts-Objekt) | `memfuse-core` | ❌ Nicht implementiert |
| `CausalEdge` (vierte Graph-Dimension) | `memfuse-core`/`memfuse-graph` | ❌ Nicht implementiert |
| PathRAG-Pfad-Extraktion als dritte `GraphTraversalStrategy` | `memfuse-core` | ❌ Nicht implementiert — Enum hat weiterhin nur `Hops`/`PersonalizedPageRank` |
| Kalibriertes Kaskaden-Routing (statt statischem `1.2×`-Boost) | `memfuse-router` | ❌ Nicht implementiert — `router.rs` nutzt weiterhin unkalibrierten `1.2`-Faktor |
| Kryptographischer Löschbeweis (Verified Forgetting) | `memfuse-crypto` | ❌ Nicht implementiert |
| MCP Schreibautorisierungs-Gate | `memfuse-mcp` | ❌ Nicht implementiert |
| `memfuse-quant` (Matryoshka-Truncation, Quantisierung) | Neues Crate | ❌ Nicht implementiert |
| `memfuse-kv` (Retrieval↔Inferenz-Brücke, KV-Cache) | Neues Crate | ❌ Nicht implementiert |
| `VamanaIndex` (Disk-resident ANN) | `memfuse-index` | ❌ Nicht implementiert |
| `IoBackend`-Abstraktion (io_uring) | `memfuse-store` | ❌ Nicht implementiert |
| `MemFuseErrorCode` (`#[repr(i32)]` stabiler Fehlercode) | `memfuse-core` | ❌ Nicht implementiert (unterscheidet sich von `MemFuseErrorDto`, das bereits existiert und die FFI-Grenzen korrekt bedient) |
| Cache-bewusste Segmenttrennung im `ContextCompactor` | `memfuse-db` | ❌ Nicht implementiert |
| Sleep-Cycle-Konsolidierung als `AgentTool` | `memfuse-agent` | ❌ Nicht implementiert |

Diese Punkte werden in Abschnitt 3 als **niedrig priorisierte, optionale Zukunfts-Prompts** aufgenommen — sie sind technisch fundiert, aber kein Bugfix und sollten nach den P0/P1-Punkten angegangen werden.

---

## 2. Priorisierte Gesamtliste der offenen Punkte

| Prio | ID | Befund | Aufwand | Quelle |
|---|---|---|---|---|
| 🔴 P0 | F-01 | `NamespaceViolation` toter Code — entweder verdrahten oder entfernen | Klein–Mittel | Fix-Plan 1.2 |
| 🔴 P0 | F-02 | ADR-028 (MemoryType-Klassifikation) fehlt in `DECISIONS.md` | Trivial | P-03 |
| 🟠 P1 | F-03 | `scan_prefix_at`-Default-Fehlertyp-Inkonsistenz (`PolicyViolation` vs. `CapabilityUnsupported`) | Klein | H-4 |
| 🟠 P1 | F-04 | `next_tx()`/`allocate_tx()`-Redundanz auflösen | Klein | H-3 |
| 🟠 P1 | F-05 | Community-Detection-Proptests + `PprConfig.warn_on_non_convergence` + Sink-Node-Verifikation | Mittel | P-07 |
| 🟠 P1 | F-06 | `delete_prefix`-Default-Trait-Impl durch Batch-Tombstone ersetzen | Klein–Mittel | Fix-Plan 3.x |
| 🟡 P2 | F-07 | Blake3 im MemTable-Hot-Path durch AHash ersetzen | Klein | H-1 |
| 🟡 P2 | F-08 | NaN-Validierung aus Distance-Hot-Path entfernen (Insert-Zeit-Check genügt) | Klein | H-5 |
| 🟡 P2 | F-09 | Dual Entry-Point HNSW vereinheitlichen | Mittel | H-6 |
| 🟡 P2 | F-10 | `WorkflowState::graph_hash` von `String` zu `[u8; 32]` | Klein | H-8 |
| 🟡 P2 | F-11 | CSR `compact()` Incremental statt Full-Rebuild | Groß | H-9 |
| 🟡 P2 | F-12 | InvertedIndex Namespace-Kollision "default" beheben | Klein | H-10 |
| 🟡 P2 | F-13 | `rebuild_threshold` umbenennen zu `min_connectivity_ratio` | Klein | M-1 |
| 🟡 P2 | F-14 | `unwrap_or_else(\|\| panic!(...))` in Compaction entfernen | Klein | M-2 |
| 🟢 P3 | F-15 | BM25-IDF-Floor `1e-6` überprüfen/auf `0.0` ändern | Klein | M-3 |
| 🟢 P3 | F-16 | ScalarQuantizer automatischen Recalibration-Trigger ergänzen | Mittel | M-5 |
| 🟢 P3 | F-17 | `MemTable::get()` auf `max_by_key`-basierte Seq-Order umstellen | Klein | M-6 |
| 🟢 P3 | F-18 | Deutsche Token-Kalibrierung in `estimate_tokens()` | Klein–Mittel | Fix-Plan 2.7 |
| 🔵 P4 | F-19 | Zero-Copy Clone-Reduktion L3→L5 (P-08 vollständig nachholen) | Groß | P-08 |
| 🔵 P4 | F-20 | Kalibriertes Kaskaden-Routing in `memfuse-router` | Mittel | Source-of-Truth-Strategie |
| 🔵 P4 | F-21 | `ProvenanceRecord` als abfragbares Herkunfts-Objekt | Groß | Interface-Spec §1.1 |

Die Prompts für F-01 bis F-18 sind vollständig in Abschnitt 3 ausformuliert (konkrete Bugfixes/Hygiene, hohe Nutzen/Aufwand-Ratio). F-19 bis F-21 sind als kompaktere Prompts am Ende beigefügt, da sie größere strategische Entscheidungen voraussetzen.

---

## 3. Die Jules-Prompts

---

### F-01 — `NamespaceViolation`: Verdrahten oder entfernen (Architekturentscheidung + Umsetzung)

```markdown
ROLLE / PERSONA:
Du bist ein Principal Rust API-Architekt mit Erfahrung im Aufräumen von
Error-Enum-Drift zwischen Kernbibliothek und mehreren FFI-Außengrenzen
(Python, MCP, Tauri). Du weißt, wie teuer unbenutzte Error-Varianten für
die Wartbarkeit sind: jede FFI-Grenze muss sie weiterhin mappen, obwohl sie
nie erzeugt werden, was Testabdeckung und Lesbarkeit gleichermaßen verwässert.

REPOSITORY: https://github.com/tfufuz1/memfuse
CRATES: memfuse-core (Entscheidung), optional memfuse-db (Verdrahtung)

VERIFIZIERTER IST-ZUSTAND (Stand 2026-08-30):
- `MemFuseError::NamespaceViolation(String)` ist deklariert in
  `crates/memfuse-core/src/error.rs`.
- Sie wird gemappt in `error_dto.rs`, `memfuse-py/src/lib.rs` (Zeile 255,
  als `PyPermissionError`) und `memfuse-mcp/src/protocol.rs` (Zeile 117).
- Sie wird an KEINER Stelle im gesamten Repository tatsächlich erzeugt
  (`grep -rn "NamespaceViolation(" --include="*.rs" .` zeigt außerhalb von
  `error.rs`/`error_dto.rs`/`protocol.rs`/`memfuse-py/src/lib.rs` NULL
  Treffer). Die Datei `crates/memfuse-db/src/namespace.rs`, die ursprünglich
  Multi-Tenancy-Namespace-Isolation implementieren sollte, existiert nicht
  mehr im Repository.

AUFGABE — Triff zuerst die Architekturentscheidung, dokumentiere sie, dann setze um:

=== SCHRITT 1: Bestandsaufnahme ===
1.1 Bestätige per `grep -rn "NamespaceViolation" --include="*.rs" .`, dass die
    obige Analyse noch zutrifft (Code kann sich seit diesem Prompt verändert
    haben — prüfe live, nicht blind).
1.2 Prüfe `docs/memfuse_strategic_roadmap.md` und `MEMFUSE_SOURCE_OF_TRUTH_STRATEGY.md`
    (falls im Repo vorhanden) auf Hinweise, ob Abteilungs-/Mandanten-Isolation
    (z. B. HR vs. Vertrieb vs. Geschäftsleitung als getrennte, gegeneinander
    abgeschottete Collections) ein geplantes Feature ist.

=== SCHRITT 2a: FALLS Namespace-Isolation gebraucht wird (echte Verdrahtung) ===
2a.1 Führe eine minimal-invasive Namespace-Prüfung in `Collection` ein:
     - Neues optionales Feld `allowed_namespace: Option<String>` in der
       Collection-Konfiguration oder ein separates `NamespacePolicy`-Objekt,
       das bei jedem `insert()`/`hybrid_search()`/`get()` geprüft wird.
     - Bei Verstoß: `Err(MemFuseError::NamespaceViolation(format!(
       "Access denied: collection '{}' is not in allowed namespace '{}'",
       collection_name, namespace)))`
2a.2 Schreibe einen Test, der beweist, dass ein Cross-Namespace-Zugriff
     tatsächlich `NamespaceViolation` auslöst:
     ```rust
     #[tokio::test]
     async fn test_cross_namespace_access_denied() {
         // Collection mit allowed_namespace = "hr" erstellen
         // Zugriff mit namespace = "finance" versuchen
         // Erwarte Err(MemFuseError::NamespaceViolation(_))
     }
     ```
2a.3 Dokumentiere die Entscheidung als neuen ADR-Eintrag in `DECISIONS.md`
     (nächste freie Nummer ermitteln — prüfe die höchste existierende
     ADR-Nummer per `grep -oP '(?<=## ADR-)\d+' DECISIONS.md | sort -n | tail -1`).

=== SCHRITT 2b: FALLS Namespace-Isolation NICHT gebraucht wird (Entfernung) ===
2b.1 Entferne die Variante `NamespaceViolation` aus
     `crates/memfuse-core/src/error.rs` (inkl. aller Tests, die sie
     referenzieren, Zeilen ~235 und Umgebung).
2b.2 Entferne den zugehörigen Mapping-Arm in `crates/memfuse-core/src/error_dto.rs`
     (Zeile ~105 und Testfall Zeile ~263-264).
2b.3 Entferne den Mapping-Arm in `crates/memfuse-py/src/lib.rs` (Zeile ~255)
     — behalte nur `"PolicyViolation"` im selben Match-Arm, falls dort
     zusammengefasst.
2b.4 Entferne den Mapping-Arm in `crates/memfuse-mcp/src/protocol.rs` (Zeile ~117).
2b.5 Führe `cargo build --workspace` (bzw. äquivalente statische Prüfung)
     aus, um sicherzustellen, dass kein verwaister Import oder `unreachable
     pattern`-Warning entsteht.

=== SCHRITT 3: Empfehlung ===
Falls unklar, welcher Pfad (2a oder 2b) zutrifft: Diese Codebasis hat
BEREITS ein funktionierendes Isolationsmuster auf Storage-Ebene
(`memfuse-crypto` Namespace-Isolation, siehe `crates/memfuse-crypto/tests/namespace_isolation.rs`)
sowie separate Collections als De-facto-Mandantentrennung. Die pragmatische
Empfehlung — sofern die vorherige Analyse in Schritt 1.2 keinen aktiven
Bedarf für API-Level-Namespace-Enforcement zeigt — ist **Pfad 2b
(Entfernung)**, da tote Error-Varianten an FFI-Grenzen Wartungslast ohne
Nutzen erzeugen. Triff die Entscheidung selbst begründet und dokumentiere
sie im Abschlussbericht.

DEFINITION OF DONE:
- [ ] Entscheidung (2a oder 2b) getroffen und im Abschlussbericht begründet
- [ ] Bei 2a: NamespacePolicy implementiert, Test grün, ADR-Eintrag ergänzt
- [ ] Bei 2b: Alle vier Fundstellen bereinigt, kein toter Code mehr,
      `cargo build --workspace` sauber
- [ ] WORKING_STATE.md aktualisiert
```

---

### F-02 — ADR-028 für MemoryType-Klassifikation nachtragen

```markdown
ROLLE / PERSONA:
Du bist ein Technical Writer mit tiefem Rust-Hintergrund, spezialisiert auf
Architecture Decision Records (ADRs) für Datenbank- und Memory-System-Projekte.

REPOSITORY: https://github.com/tfufuz1/memfuse
DATEI: DECISIONS.md (Root)

VERIFIZIERTER IST-ZUSTAND:
- Der `MemoryType`-Enum (Episodic/Semantic/Procedural/Working) inkl.
  `default_decay()`, `default_ttl_tx()`, `as_metadata_key()` ist vollständig
  implementiert in `crates/memfuse-core/src/types/domain.rs` (ab Zeile ~538).
- `Collection::insert_typed()` (crates/memfuse-db/src/collection/crud.rs:138)
  und `MemFuse::insert_typed()` (crates/memfuse-db/src/lib.rs:598) sind
  implementiert und exponiert.
- `extract_memory_type()` in `crates/memfuse-db/src/filter.rs:13` ist implementiert.
- `docs/TYPE_REGISTRY.md` Zeile 23 referenziert bereits `MemoryType`.
- Der ursprünglich für diese Arbeit vorgesehene Eintrag "## ADR-028:
  Kognitive Gedächtnistypen-Klassifikation" fehlt in `DECISIONS.md` — die
  Nummer ADR-028 ist bereits durch ein anderes Thema belegt
  ("Dezentrales Inline-Kontextsystem, Sekundengenaue Zeitstempel & ...").

AUFGABE:

1. Ermittle die höchste aktuell vergebene ADR-Nummer in DECISIONS.md:
   ```bash
   grep -oP '(?<=## ADR-)\d+' DECISIONS.md | sort -n | tail -1
   ```
2. Füge am Ende von DECISIONS.md einen neuen ADR-Eintrag unter der
   nächsten freien Nummer hinzu (NICHT ADR-028 wiederverwenden, da belegt).
   Nutze folgenden Text als Grundlage, angepasst an die im Repo übliche
   ADR-Formatierung (prüfe 2-3 bestehende ADR-Einträge auf Stil/Struktur
   und übernimm das Muster):

   ```markdown
   ## ADR-0XX: Kognitive Gedächtnistypen-Klassifikation (MemoryType)

   **Status**: Angenommen
   **Datum**: 2026-08-29 (Implementierung) / 2026-08-30 (Dokumentation nachgetragen)

   ### Kontext
   Die strategische Roadmap (Phase 2) forderte eine explizite Klassifikation
   gespeicherter Einträge nach kognitivem Gedächtnistyp (Vorbilder: MemOS/
   MemCube, Mem0, A-MEM): episodisch (Ereignisse), semantisch (Fakten),
   prozedural (Workflows) und operativ (Working Memory, Session-Kontext).
   Bisher wurden alle Dokumente uniform behandelt, ohne dass Retrieval-
   Strategie oder Lifecycle (Decay, TTL) von der Art des Inhalts abhingen.

   ### Entscheidung
   Ein neuer `#[non_exhaustive]` Enum `MemoryType` in `memfuse-core` mit
   vier Varianten (Episodic, Semantic [Default], Procedural, Working).
   Jede Variante liefert über `default_decay()` eine passende
   `DecayFunction` (Episodic: Exponential mit 10.000 TX Halbwertszeit;
   Semantic: keine Decay; Procedural: StepFloor, verstärkt durch Nutzung;
   Working: sehr schnelle Exponential-Decay mit 500 TX Halbwertszeit) und
   über `default_ttl_tx()` eine optionale TTL (nur Working Memory: 50.000 TX).
   Der Typ wird additiv über `Collection::insert_typed()` gesetzt und als
   `"memory_type"`-Feld in den Dokument-Metadaten persistiert — bestehende
   Dokumente ohne dieses Feld werden rückwärtskompatibel als `Semantic`
   interpretiert (`extract_memory_type()`).

   ### Konsequenzen
   - Additiv, keine Breaking Changes an bestehenden `insert()`-Aufrufern.
   - `trigger_reaper()` nutzt die typspezifische Decay-Function für einen
     aktiven TxId-basierten Sweep (siehe zugehörige Reaper-Härtung).
   - Zukünftige Retrieval-Strategien können nach `MemoryType` filtern oder
     gewichten (noch nicht implementiert, aber durch additive Enum-Erweiterung
     vorbereitet).

   ### Alternativen erwogen
   - Freitext-Tag statt Enum: verworfen, da keine Typsicherheit und keine
     automatische Decay-/TTL-Kopplung möglich gewesen wäre.
   ```

3. Aktualisiere `docs/TYPE_REGISTRY.md`, falls der bestehende Eintrag
   (Zeile 23) nicht bereits auf den neuen ADR verweist — ergänze eine
   Spalte oder einen Hinweis "Siehe ADR-0XX".

DEFINITION OF DONE:
- [ ] Neuer ADR-Eintrag mit korrekter, noch nicht vergebener Nummer in
      DECISIONS.md
- [ ] Eintrag folgt dem bestehenden Formatierungsmuster des Dokuments
- [ ] docs/TYPE_REGISTRY.md verweist auf den neuen ADR
- [ ] Keine Code-Änderungen nötig — reine Dokumentationsarbeit
```

---

### F-03 — `scan_prefix_at`-Default-Fehlertyp konsistent machen

```markdown
ROLLE / PERSONA:
Du bist ein Principal Rust Trait-Design-Experte mit Fokus auf konsistente
Fehlersemantik über Default-Trait-Implementierungen hinweg, insbesondere in
Systemen mit einem `capability_coverage`-Testvertrag, der garantiert, dass
jede `*_at`-Methode (Snapshot-Isolation) ein einheitliches Fehlerverhalten hat.

REPOSITORY: https://github.com/tfufuz1/memfuse
CRATE: memfuse-core

VERIFIZIERTER IST-ZUSTAND:
- `crates/memfuse-core/src/traits.rs`, Methode `scan_prefix_at()` (Zeile ~184):
  Default-Implementierung gibt `Err(MemFuseError::PolicyViolation(...))` zurück.
- ALLE anderen `*_at`-Default-Implementierungen im selben Trait-File
  (`VectorIndex::search_at`, `TextIndex::search_at`, `GraphIndex::traverse_at`,
  `GraphIndex::traverse_at_time`) geben stattdessen
  `Err(MemFuseError::capability_unsupported("snapshot_read_at", "..."))` zurück.
- Diese Inkonsistenz ist funktional nicht neutral: `capability_unsupported`
  ist als strukturierter, mit einer Capability-ID versehener Fehler gedacht
  (nutzbar für Feature-Discovery durch Aufrufer), während `PolicyViolation`
  semantisch "Zugriff verweigert" statt "Feature fehlt" signalisiert.
- `LsmStorage` in `crates/memfuse-store/src/lsm.rs` implementiert
  `scan_prefix_at()` bereits vollständig (siehe P-06/WP14) — diese Aufgabe
  betrifft NUR den Default-Fall für andere/zukünftige `StorageEngine`-
  Implementierungen, die die Methode nicht überschreiben.

AUFGABE:

1. Analysiere den `capability_unsupported()`-Konstruktor in
   `crates/memfuse-core/src/error.rs`, um die exakte Signatur zu verstehen
   (Capability-Name-String + Beschreibung).

2. Ändere die Default-Implementierung von `scan_prefix_at()` in
   `crates/memfuse-core/src/traits.rs`:

   ```rust
   // VORHER:
   async fn scan_prefix_at(
       &self,
       _prefix: &[u8],
       _seq_no: u64,
   ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
       Err(crate::error::MemFuseError::PolicyViolation(
           "scan_prefix_at must be explicitly implemented to guarantee snapshot isolation".into(),
       ))
   }

   // NACHHER:
   async fn scan_prefix_at(
       &self,
       _prefix: &[u8],
       _seq_no: u64,
   ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
       Err(crate::error::MemFuseError::capability_unsupported(
           "snapshot_read_at",
           "Storage-level snapshot-isolated prefix scan (scan_prefix_at) is not supported by default — implementors must override this method to guarantee MVCC snapshot isolation.",
       ))
   }
   ```

3. Prüfe alle Aufrufer dieser Default-Methode (falls es Test-Stubs oder
   Placeholder-`StorageEngine`-Implementierungen gibt, die auf
   `PolicyViolation` matchen), und passe sie auf `CapabilityUnsupported`
   an, falls sie den alten Fehlertyp explizit erwarten:
   ```bash
   grep -rn "scan_prefix_at" --include="*.rs" . | grep -i "PolicyViolation"
   ```

4. Ergänze/aktualisiere den `capability_coverage`-Testmodul-Test für
   `scan_prefix_at`, damit er analog zu den bereits bestehenden Tests für
   `search_at`/`traverse_at` denselben Fehlertyp-Vertrag prüft:
   ```rust
   #[tokio::test]
   async fn test_scan_prefix_at_default_returns_capability_unsupported() {
       // Placeholder-StorageEngine ohne Override von scan_prefix_at()
       let result = placeholder.scan_prefix_at(b"prefix", 0).await;
       assert!(matches!(
           result,
           Err(MemFuseError::CapabilityUnsupported { .. })
       ));
   }
   ```

DEFINITION OF DONE:
- [ ] `scan_prefix_at()`-Default nutzt `capability_unsupported()` statt `PolicyViolation`
- [ ] Kein bestehender Test bricht (insbesondere `LsmStorage`s eigene
      Implementierung ist davon nicht betroffen, da sie den Default überschreibt)
- [ ] Neuer/aktualisierter `capability_coverage`-Test beweist Konsistenz
- [ ] cargo test --package memfuse-core — PASS
- [ ] WORKING_STATE.md aktualisiert
```

---

### F-04 — `next_tx()`/`allocate_tx()`-Redundanz auflösen

```markdown
ROLLE / PERSONA:
Du bist ein Senior Rust API-Kurator mit Erfahrung im schrittweisen
Deprecaten redundanter öffentlicher Methoden ohne Breaking Changes in
aktiv genutzten Downstream-Crates.

REPOSITORY: https://github.com/tfufuz1/memfuse
CRATES: memfuse-db (Definition), memfuse-checkpoint (Aufrufer)

VERIFIZIERTER IST-ZUSTAND:
- `crates/memfuse-db/src/collection/tx.rs` definiert sowohl `next_tx()`
  (Zeile 7) als auch `allocate_tx()` (Zeile 20) — beide Methoden sind
  IDENTISCH: `self.next_tx.fetch_add(1, Ordering::SeqCst)` mit identischer
  `TxId::INTERNAL_BASE`-Overflow-Prüfung und identischer Fehlermeldung.
- `next_tx()` ist weiterhin `pub` (nicht `pub(crate)`).
- `next_tx()` wird extern aufgerufen in: `crates/memfuse-checkpoint/src/lib.rs`
  (Zeilen 340, 377, 960 — inkl. eines Tests), sowie intern in
  `crates/memfuse-db/src/collection/maintenance.rs` (Zeilen 36, 138, 517)
  und `crates/memfuse-db/src/collection/mod.rs` (Zeilen 394, 427).
- `allocate_tx()` ist die "offizielle" öffentliche API laut Doc-Kommentar
  ("Externe Crates verwenden diese Methode statt eigener TxId-Generierung").

AUFGABE:

1. Analysiere ALLE Aufrufstellen von `next_tx()` im gesamten Workspace:
   ```bash
   grep -rn "\.next_tx()" --include="*.rs" . | grep -v target
   ```
   Unterscheide zwischen Aufrufern innerhalb von `memfuse-db` selbst
   (können auf `allocate_tx()` umgestellt werden ohne API-Bruch) und
   externen Aufrufern (memfuse-checkpoint — API-Bruch nur mit Migration).

2. Vereinheitliche auf `allocate_tx()` als einzige öffentliche Methode:
   a) Ersetze alle internen Aufrufe von `self.next_tx()` in
      `collection/maintenance.rs` und `collection/mod.rs` durch
      `self.allocate_tx()`.
   b) Ersetze alle Aufrufe in `memfuse-checkpoint/src/lib.rs`
      (`store.next_tx()`) durch `store.allocate_tx()`.
   c) Markiere `next_tx()` als deprecated statt sie sofort zu entfernen
      (Breaking-Change-Vermeidung für externe Nutzer der Bibliothek, die
      diese Doku evtl. nicht kennen):
      ```rust
      #[deprecated(since = "0.X.0", note = "Use `allocate_tx()` instead — both methods are functionally identical, `allocate_tx()` is the canonical public API.")]
      pub fn next_tx(&self) -> Result<TxId> {
          self.allocate_tx()
      }
      ```
      Ändere den Körper von `next_tx()` so, dass er intern `allocate_tx()`
      aufruft (Single Source of Truth statt Code-Duplikation), auch wenn
      die Methode deprecated bleibt.

3. Aktualisiere den Test in `crates/memfuse-checkpoint/tests/` bzw.
   `src/lib.rs` (Zeile ~960), der `store.next_tx()` aufruft, auf
   `store.allocate_tx()`, um keine Deprecation-Warnung im eigenen
   Testcode zu erzeugen.

4. Prüfe `crates/memfuse-db/src/collection/tests.rs` (Zeilen 682-684),
   die explizit `col.next_tx()` in einem Test mit dem Kommentar
   "unwrap allowed" nutzen — entscheide, ob dieser Test die Deprecation
   testen soll (dann `#[allow(deprecated)]` ergänzen) oder auf
   `allocate_tx()` umgestellt werden soll (bevorzugt, falls der Test
   nicht spezifisch `next_tx()`s Verhalten prüft).

DEFINITION OF DONE:
- [ ] `next_tx()` ist als `#[deprecated]` markiert und delegiert intern an `allocate_tx()`
- [ ] Alle internen Aufrufer (memfuse-db, memfuse-checkpoint) nutzen `allocate_tx()`
- [ ] Keine Deprecation-Warnings im eigenen Workspace-Build
- [ ] cargo build --workspace — sauber (keine neuen Warnings)
- [ ] cargo test --package memfuse-db --package memfuse-checkpoint — PASS
- [ ] WORKING_STATE.md aktualisiert
```

---

### F-05 — PPR/Community-Detection: Proptest-Lücke schließen + `warn_on_non_convergence`

```markdown
ROLLE / PERSONA:
Du bist ein Principal Rust Graph-Algorithmen-Ingenieur mit Expertise in
Power-Iteration-Konvergenz-Beweisen und Label-Propagation-Determinismus.
Du hast in TigerGraph/Neo4j-ähnlichen Systemen Proptest-Suiten für
PageRank- und Community-Detection-Implementierungen aufgebaut, die
gezielt adversariale Graphstrukturen (Zyklen, Sink-Nodes, disconnected
Components, Cliquen) erzeugen.

REPOSITORY: https://github.com/tfufuz1/memfuse
CRATES: memfuse-core (PprConfig), memfuse-graph (ppr.rs, community.rs)

VERIFIZIERTER IST-ZUSTAND (P-07-Nacharbeit):
- `PprConfig` in `crates/memfuse-core/src/types/domain.rs` (Zeile ~600) hat
  aktuell drei Felder: `damping_factor`, `max_iterations`, `convergence_epsilon`.
  Das geforderte vierte Feld `warn_on_non_convergence: bool` FEHLT.
- `crates/memfuse-graph/src/ppr.rs` hat bereits einen Proptest
  `prop_ppr_rank_mass_conservation` (Zeile ~604), der zufällige Graphen
  erzeugt und beweist, dass die PPR-Score-Summe ≈ 1.0 ist — das deckt
  funktional bereits "nie Panic" (durch `.unwrap()` im Testkörper selbst,
  der bei jedem Fehlschlag fehlschlagen würde) und "Summe ≈ 1.0" ab.
  DIESER Proptest ist ausreichend für die PPR-Seite — hier ist NUR das
  `warn_on_non_convergence`-Feld nachzuholen.
- `crates/memfuse-graph/src/community.rs` hat DREI konventionelle Tests
  (`test_community_detection_determinism`,
  `test_community_detection_disconnected_clusters`,
  `test_community_detection_non_convergence_logs_warning_and_returns_best_effort`),
  aber KEINE proptest!-Blöcke. Das bedeutet: Determinismus ist für die
  konkreten Testfälle bewiesen, aber nicht für eine breite, zufällig
  generierte Menge an Graphstrukturen (Zyklen, Cliquen, Sink-Nodes in
  beliebiger Kombination).
- Sink-Node-Handling (Knoten ohne ausgehende Kante, die ihre PPR-Masse
  korrekt an den Restart-Vektor zurückgeben müssen) ist im Code nicht
  unter diesem Namen auffindbar (`grep -in "sink" crates/memfuse-graph/src/ppr.rs`
  liefert keine Treffer) — verifiziere, ob es implizit durch die
  bestehende Massen-Erhaltungs-Property bereits korrekt gehandhabt wird,
  oder ob es eine echte Lücke ist.

AUFGABEN:

=== AUFGABE 1: `warn_on_non_convergence`-Feld ergänzen ===

1.1 Erweitere `PprConfig`:
    ```rust
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    pub struct PprConfig {
        pub damping_factor: f32,
        pub max_iterations: u32,
        pub convergence_epsilon: f32,
        /// Gibt eine nicht-konvergierte Warnung (tracing::warn!) aus, wenn
        /// max_iterations erreicht wird, bevor convergence_epsilon
        /// unterschritten wurde. Kein Fehler — die Berechnung liefert das
        /// beste bisher erreichte Ergebnis zurück. Default: true.
        #[serde(default = "default_warn_on_non_convergence")]
        pub warn_on_non_convergence: bool,
    }

    fn default_warn_on_non_convergence() -> bool { true }

    impl Default for PprConfig {
        fn default() -> Self {
            Self {
                damping_factor: 0.85,
                max_iterations: 100,
                convergence_epsilon: 1e-6,
                warn_on_non_convergence: true,
            }
        }
    }
    ```
    Nutze `#[serde(default = ...)]` statt eines einfachen Default-Derives
    auf Feldebene, damit bestehende, bereits persistierte JSON-Configs
    (falls vorhanden) ohne dieses Feld weiterhin deserialisierbar bleiben
    (Rückwärtskompatibilität).

1.2 Verdrahte das Feld in `personalized_page_rank()` in
    `crates/memfuse-graph/src/csr.rs`: Finde die Stelle, an der die
    Power-Iteration terminiert (entweder durch Konvergenz oder durch
    Erreichen von `max_iterations`), und füge einen bedingten
    `tracing::warn!`-Aufruf hinzu, wenn `max_iterations` erreicht wurde,
    OHNE dass `convergence_epsilon` unterschritten wurde UND
    `config.warn_on_non_convergence == true`:
    ```rust
    if iterations_used >= config.max_iterations && !converged && config.warn_on_non_convergence {
        tracing::warn!(
            max_iterations = config.max_iterations,
            final_delta = last_delta,
            "PPR did not converge within max_iterations — returning best-effort result"
        );
    }
    ```

1.3 Test:
    ```rust
    #[tokio::test]
    async fn test_ppr_warn_on_non_convergence_suppressible() {
        // Config mit sehr niedrigem max_iterations (z.B. 1) und
        // warn_on_non_convergence: false auf einem Graph, der garantiert
        // nicht in 1 Iteration konvergiert.
        // Test kann nicht direkt auf tracing::warn! prüfen ohne Subscriber-
        // Mock — validiere stattdessen, dass die Funktion trotzdem ein
        // Ok(Vec<...>)-Ergebnis liefert (kein Error, kein Panic) unabhängig
        // vom warn_on_non_convergence-Wert.
    }
    ```

=== AUFGABE 2: Community-Detection-Proptests ergänzen ===

2.1 Füge zu `crates/memfuse-graph/src/community.rs` einen `proptest!`-Block
    hinzu (nutze dasselbe Muster wie `prop_ppr_rank_mass_conservation` in
    `ppr.rs` als Vorlage für Graph-Erzeugung via `CsrGraph`):

    ```rust
    proptest::proptest! {
        #[test]
        fn prop_community_detection_never_panics(
            node_count in 1usize..50,
            edge_specs in proptest::collection::vec((0..50usize, 0..50usize), 0..150),
            max_iterations in 1u32..50,
        ) {
            let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
            let res: Result<(), proptest::test_runner::TestCaseError> = rt.block_on(async {
                let graph = CsrGraph::new();
                let tx = TxId::new(1);
                for i in 0..node_count {
                    graph.add_entity(tx, Entity::new(EntityId::new(i as u64 + 1), format!("N{i}"), "Node")).await.unwrap();
                }
                for (src, dst) in edge_specs {
                    let src_id = EntityId::new((src % node_count) as u64 + 1);
                    let dst_id = EntityId::new((dst % node_count) as u64 + 1);
                    // Selbst-Loops und Duplikate bewusst zulassen (adversarial)
                    let _ = graph.add_edge(tx, Edge::new(src_id, dst_id, "link")).await;
                }
                graph.commit(tx).await.unwrap();

                let config = CommunityDetectionConfig { max_iterations, ..Default::default() };
                let result = graph.detect_communities(&config).await;

                // Darf NIEMALS Err(Internal-Panic-artiger Fehler) liefern —
                // ein sauberes Err bei Kapazitätsgrenzen ist ok, ein Panic nicht.
                prop_assert!(result.is_ok() || matches!(result, Err(_)));
                if let Ok(assignments) = result {
                    // Jeder eingefügte Knoten muss genau eine Zuweisung erhalten
                    prop_assert_eq!(assignments.len(), node_count);
                }
                Ok(())
            });
            res?;
        }
    }
    ```

2.2 Passe die genaue Signatur von `detect_communities()`/`CommunityDetectionConfig`
    an das tatsächliche, existierende API an (prüfe die echte Signatur vor
    dem Schreiben des Tests — die obige ist ein Gerüst, kein Copy-Paste-Fertigcode).

=== AUFGABE 3: Sink-Node-Verifikation ===

3.1 Schreibe einen gezielten (nicht Proptest-)Test, der explizit einen
    Sink-Node (Knoten ohne ausgehende Kante) in den Graphen einfügt und
    prüft, dass die PPR-Massen-Erhaltung (Summe ≈ 1.0) auch dann noch gilt:
    ```rust
    #[tokio::test]
    async fn test_ppr_handles_sink_node_correctly() {
        // Graph: A -> B, B hat KEINE ausgehende Kante (Sink), C -> A
        // PPR von Seed A: Summe aller Scores muss weiterhin ≈ 1.0 sein,
        // insbesondere darf B's "verlorene" Masse nicht einfach verschwinden.
    }
    ```
3.2 Falls dieser Test fehlschlägt (echte Sink-Node-Lücke): Implementiere
    Standard-PPR-Sink-Handling — nicht-ausgehende Masse eines Sink-Knotens
    wird gleichmäßig auf den Restart-Vektor (Seed-Knoten) redistribuiert,
    statt implizit zu verschwinden. Falls der Test bereits grün ist:
    Dokumentiere im Abschlussbericht, dass die bestehende Implementierung
    Sink-Nodes bereits korrekt handhabt (kein Fix nötig), und behalte den
    Test als Regressionsschutz.

DEFINITION OF DONE:
- [ ] PprConfig.warn_on_non_convergence Feld mit Serde-Default ergänzt
- [ ] Warn-Log bei Nicht-Konvergenz verdrahtet und schaltbar
- [ ] Mindestens 2 neue Proptests in community.rs (never_panics,
      every_node_assigned)
- [ ] Expliziter Sink-Node-Test in ppr.rs, Ergebnis (Fix nötig oder
      bereits korrekt) im Abschlussbericht dokumentiert
- [ ] cargo test --package memfuse-graph — PASS
- [ ] WORKING_STATE.md aktualisiert
```

---

### F-06 — `delete_prefix`-Default-Trait-Implementierung durch Batch-Tombstone ersetzen

```markdown
ROLLE / PERSONA:
Du bist ein Senior Rust Storage-Engine-Entwickler mit Erfahrung in
Batch-Mutation-APIs für Transaktionssysteme. Du kennst den Unterschied
zwischen N sequenziellen Lock-Akquisitionen und einer einzigen
Batch-Operation unter einem Lock, insbesondere bei potenziell großen
Prefix-Scans (tausende Keys).

REPOSITORY: https://github.com/tfufuz1/memfuse
CRATE: memfuse-core (Default-Trait-Methode)

VERIFIZIERTER IST-ZUSTAND:
- `crates/memfuse-core/src/traits.rs`, `delete_prefix()` Default-Impl
  (Zeile ~134): Iteriert `matching_keys` und ruft für jeden Key einzeln
  `self.delete(tx_id, &key).await?` auf — N einzelne Aufrufe.
- `crates/memfuse-store/src/lsm.rs`, `LsmStorage::delete_prefix()`
  (Zeile ~677): Ist BEREITS korrekt als echter Batch implementiert — nutzt
  `self.tx_buffer.stage_many(tx_id, ops)` mit vorbereiteter `Vec<IndexOp<...>>`,
  ein einziger Lock-Zugriff für die gesamte Batch. DIESE Implementierung
  ist NICHT betroffen und dient als Vorlage.
- Diese Aufgabe betrifft NUR den Default-Fall im Trait für zukünftige/
  andere `StorageEngine`-Implementierungen, die `delete_prefix()` nicht
  explizit überschreiben.

AUFGABE:

1. Prüfe, ob `TxBuffer::stage_many()` (oder ein äquivalenter Batch-Mechanismus)
   generisch genug im `StorageEngine`-Trait selbst verfügbar ist, oder ob
   er nur in der konkreten `LsmStorage`-Implementierung existiert.
   `grep -n "stage_many\|fn delete\b" crates/memfuse-core/src/traits.rs`

2. Falls der Trait selbst KEINEN generischen Batch-Mechanismus exponiert
   (wahrscheinlich, da `stage_many` ein `LsmStorage`-internes Detail sein
   könnte): Ergänze eine neue, generische `delete_many()`-Methode zum
   `StorageEngine`-Trait mit sinnvollem Default (sequenzielle Delegation
   an `delete()`, aber explizit als Performance-Hinweis dokumentiert):

   ```rust
   /// Deletes multiple keys as a single logical batch operation.
   ///
   /// # Performance
   /// Default implementation delegates to sequential `delete()` calls.
   /// Implementors handling large batches (e.g. from `delete_prefix()`)
   /// SHOULD override this with a true batch operation (single lock
   /// acquisition) to avoid per-key lock contention.
   async fn delete_many(&self, tx_id: TxId, keys: Vec<Vec<u8>>) -> Result<u64> {
       let mut deleted = 0u64;
       for key in keys {
           self.delete(tx_id, &key).await?;
           deleted += 1;
       }
       Ok(deleted)
   }
   ```

3. Ändere die Default-Implementierung von `delete_prefix()` so, dass sie
   `delete_many()` statt einer manuellen Schleife nutzt:
   ```rust
   async fn delete_prefix(&self, tx_id: TxId, prefix: &[u8]) -> Result<u64> {
       let matching_keys: Vec<Vec<u8>> = self.scan_prefix(prefix).await?
           .into_iter()
           .map(|(key, _)| key)
           .collect();
       self.delete_many(tx_id, matching_keys).await
   }
   ```
   Dies alleine löst das Grundproblem noch nicht (Default von `delete_many`
   ist weiterhin sequenziell) — ABER es schafft den Erweiterungspunkt,
   den zukünftige `StorageEngine`-Implementierungen gezielt überschreiben
   können, ohne `delete_prefix()` selbst neu schreiben zu müssen.

4. Überschreibe `delete_many()` in `LsmStorage` mit dem bereits in
   `delete_prefix()` vorhandenen Batch-Muster (Wiederverwendung des
   bestehenden `stage_many()`-Codes), damit `LsmStorage` jetzt sowohl
   `delete_prefix()` als auch das neue, generischere `delete_many()`
   effizient implementiert (Konsistenz).

5. Tests:
   ```rust
   #[tokio::test]
   async fn test_delete_many_default_impl_deletes_all_keys() {
       // Mock-StorageEngine ohne delete_many-Override
       // Ruft delete_prefix() auf, prüft dass ALLE passenden Keys weg sind
   }

   #[tokio::test]
   async fn test_lsm_storage_delete_many_uses_single_batch() {
       // Beweise (z.B. via Instrumentierung oder Lock-Contention-Zählung),
       // dass LsmStorage::delete_many() NICHT N sequenzielle delete()-Aufrufe
       // macht, sondern eine Batch-Operation
   }
   ```

DEFINITION OF DONE:
- [ ] Neue `delete_many()`-Methode im StorageEngine-Trait mit dokumentiertem
      Performance-Hinweis
- [ ] `delete_prefix()`-Default nutzt `delete_many()` intern
- [ ] `LsmStorage::delete_many()` überschrieben mit echtem Batch (kein
      Verhaltensunterschied zu vorher, nur Konsistenz-Verbesserung)
- [ ] Tests grün
- [ ] cargo test --package memfuse-core --package memfuse-store — PASS
- [ ] WORKING_STATE.md aktualisiert
```

---

### F-07 — Blake3 im MemTable-Hot-Path durch AHash ersetzen

```markdown
ROLLE / PERSONA:
Du bist ein Performance-Engineer mit Expertise in Hash-Funktions-Auswahl
für nicht-kryptographische Zwecke (Shard-Selektion, Hash-Maps). Du kennst
den Unterschied zwischen kryptographischen Hashes (Blake3, SHA-256 — für
Integrität/Sicherheit) und schnellen Nicht-Krypto-Hashes (AHash, FxHash —
für interne Datenstruktur-Verteilung), und weißt, wann welcher angebracht ist.

REPOSITORY: https://github.com/tfufuz1/memfuse
CRATE: memfuse-store

VERIFIZIERTER IST-ZUSTAND:
- `crates/memfuse-store/src/memtable.rs`, `shard_for()`-Funktion nutzt
  `blake3::hash(key)` zur Auswahl eines der 16 Shards. Blake3 ist eine
  kryptographische Hashfunktion (~1 GB/s), für reine Shard-Verteilung ohne
  Sicherheitsanforderung ist dies unnötig langsam gegenüber AHash (~10 GB/s).
- Dieser Pfad wird bei JEDEM `put()`/`get()` durchlaufen — echter Hot-Path.
- WICHTIG: Prüfe, ob `blake3` hier evtl. bewusst gewählt wurde, weil
  Determinismus über Prozessgrenzen/Instanzen hinweg gebraucht wird
  (z. B. für Tests, die exakte Shard-Zuordnung erwarten) — AHash mit
  Default-Konstruktor hat KEINEN garantierten, stabilen Seed über
  verschiedene Prozessläufe hinweg (DoS-Schutz durch Randomisierung),
  was für interne Shard-Verteilung unproblematisch ist (Verteilung muss
  nur innerhalb eines laufenden Prozesses konsistent sein), aber für
  Tests, die exakte Shard-Indizes hart-kodieren, ein Problem sein könnte.

AUFGABE:

1. Prüfe, ob `ahash` bereits eine Workspace-Dependency ist:
   `grep -n "^ahash" Cargo.toml crates/memfuse-store/Cargo.toml`
   Falls nicht: Füge sie hinzu (`ahash = "0.8"` ist zum Zeitpunkt dieses
   Prompts eine stabile, weit verbreitete Version — prüfe crates.io für
   die zum Implementierungszeitpunkt aktuelle stabile Version).

2. Prüfe alle Tests in `crates/memfuse-store/tests/` und
   `crates/memfuse-store/src/memtable.rs` selbst, die auf einen
   SPEZIFISCHEN Shard-Index für einen bestimmten Key testen
   (`grep -n "shard_for\|SHARD_COUNT" crates/memfuse-store/tests/*.rs
   crates/memfuse-store/src/memtable.rs`). Falls solche Tests existieren,
   müssen sie nach dem Hash-Wechsel angepasst werden (die konkrete
   Shard-Zuordnung für einen gegebenen Key ändert sich).

3. Ändere `shard_for()`:
   ```rust
   // VORHER:
   fn shard_for(key: &[u8]) -> usize {
       let hash = blake3::hash(key);
       // ... Ableitung aus hash.as_bytes()
   }

   // NACHHER:
   fn shard_for(key: &[u8]) -> usize {
       use std::hash::{Hash, Hasher};
       let mut hasher = ahash::AHasher::default();
       key.hash(&mut hasher);
       (hasher.finish() as usize) % SHARD_COUNT
   }
   ```
   ACHTUNG: Falls der bestehende Kommentar bei `shard_for()`
   ("long common prefixes wie '__col:hr:\0', '__docid:' müssen trotzdem
   gut verteilt werden") auf eine bewusste Wahl von Blake3 wegen dessen
   Avalanche-Eigenschaften bei ähnlichen Prefixen hinweist: Verifiziere
   VOR dem Wechsel per Mikro-Test, dass AHash bei den in der Codebasis
   tatsächlich vorkommenden Key-Präfix-Mustern (siehe `rules/tag_taxonomy.md`
   oder Grep nach Präfix-Konstanten wie `__col:`, `__graph:`, `__txt:`)
   eine vergleichbar gute Verteilung liefert — schreibe dafür einen Test,
   der 10.000 realistische Keys mit gemeinsamen Prefixen erzeugt und die
   Standardabweichung der Shard-Belegung misst.

4. Test für Verteilungsqualität:
   ```rust
   #[test]
   fn test_ahash_shard_distribution_with_common_prefixes() {
       let mut counts = [0usize; SHARD_COUNT];
       for i in 0..10_000 {
           let key = format!("__col:hr:doc-{i:08}");
           counts[MemTable::shard_for(key.as_bytes())] += 1;
       }
       let mean = 10_000.0 / SHARD_COUNT as f64;
       let variance: f64 = counts.iter()
           .map(|&c| (c as f64 - mean).powi(2))
           .sum::<f64>() / SHARD_COUNT as f64;
       let std_dev = variance.sqrt();
       // Erwartung: Standardabweichung deutlich kleiner als der Mittelwert
       // (z.B. < 15% des Mittelwerts) für akzeptable Verteilung
       assert!(std_dev < mean * 0.15, "Shard distribution too skewed: std_dev={std_dev}, mean={mean}");
   }
   ```

5. Mikro-Benchmark (optional, aber empfohlen zur Verifikation des
   Performance-Gewinns):
   ```rust
   // benches/shard_hash_bench.rs
   fn bench_shard_for_blake3_vs_ahash(c: &mut Criterion) {
       // Vorher/Nachher-Vergleich mit criterion, 10.000 realistische Keys
   }
   ```

DEFINITION OF DONE:
- [ ] `ahash` als Dependency ergänzt
- [ ] `shard_for()` nutzt `AHasher` statt `blake3::hash()`
- [ ] Verteilungs-Test beweist akzeptable Balance auch bei gemeinsamen
      Key-Präfixen
- [ ] Alle bestehenden Tests, die auf konkrete Shard-Indizes angewiesen
      waren, angepasst
- [ ] cargo test --package memfuse-store — PASS
- [ ] WORKING_STATE.md aktualisiert
```

---

### F-08 — NaN-Validierung aus Distance-Hot-Path entfernen

```markdown
ROLLE / PERSONA:
Du bist ein Performance-Engineer mit Fokus auf HNSW-Suchpfad-Optimierung.
Du verstehst, dass Eingabevalidierung so früh wie möglich (Trust Boundary)
stattfinden sollte, nicht wiederholt in jedem Hot-Path-Aufruf, sofern die
Invariante zwischen Validierungspunkt und Nutzungspunkt garantiert
aufrechterhalten wird (hier: durch Unveränderlichkeit gespeicherter Vektoren).

REPOSITORY: https://github.com/tfufuz1/memfuse
CRATES: memfuse-index

VERIFIZIERTER IST-ZUSTAND:
- `crates/memfuse-index/src/distance.rs`, `compute_distance()` (Zeile 68):
  Iteriert bei JEDEM Aufruf über ALLE Elemente beider Vektoren, um auf NaN
  zu prüfen (Zeilen 78-84), BEVOR die eigentliche Distanzberechnung erfolgt.
- `crates/memfuse-index/src/hnsw.rs`, Zeile 1027: Es gibt BEREITS einen
  NaN/Infinite-Check beim Insert eines Vektors in den Index
  (`if vector.iter().any(|x| x.is_nan() || x.is_infinite())`).
- Bei einer HNSW-Suche mit `ef=64` Kandidaten und 1536-dimensionalen
  Vektoren bedeutet das ~64 × 2 × 1536 ≈ 200.000 NaN-Vergleiche NUR für
  Validierung, die bereits beim Insert erfolgt ist — reine Redundanz im
  heißesten Pfad des gesamten Systems.
- Query-Vektoren (nicht gespeicherte Vektoren) durchlaufen `compute_distance()`
  ebenfalls — hier ist der Insert-Check NICHT anwendbar, da Query-Vektoren
  nie "inserted" werden. Der Query-Vektor muss also weiterhin an EINER
  Stelle validiert werden, aber nicht bei jedem einzelnen paarweisen Vergleich.

AUFGABE:

1. Analysiere den kompletten Aufrufpfad von `compute_distance()`:
   `grep -rn "compute_distance(" crates/memfuse-index/src/` — identifiziere
   ALLE Aufrufstellen (HNSW-Suche, Insert, Batch-Operationen, Tests).

2. Verschiebe die NaN/Infinite-Validierung von `compute_distance()`
   (Hot-Path, wird potenziell hunderttausendfach pro Suche aufgerufen)
   zu genau ZWEI Stellen:
   a) Beim Insert eines Vektors in den Index (bereits vorhanden in
      `hnsw.rs:1027` — keine Änderung nötig, dient als Beweis, dass
      gespeicherte Vektoren garantiert NaN-frei sind).
   b) Beim Empfang eines Query-Vektors am Beginn von `search()`/
      `search_at()`/`search_filtered()` in `hnsw.rs` — EINMAL pro
      Suchanfrage, nicht einmal pro paarweisem Vergleich:
      ```rust
      pub async fn search(&self, query: &[f32], k: usize) -> Result<Vec<ScoredDocument>> {
          if query.iter().any(|x| x.is_nan() || x.is_infinite()) {
              return Err(MemFuseError::invalid_input("Query vector contains NaN or infinite values"));
          }
          // ... restliche Suchlogik, die intern compute_distance() mehrfach aufruft
      }
      ```

3. Entferne die NaN-Validierungsschleife aus `compute_distance()` selbst,
   behalte aber den Dimensions-Check (`a.len() != b.len()`), da dieser
   O(1) ist und eine andere Fehlerklasse abdeckt:
   ```rust
   pub fn compute_distance(a: &[f32], b: &[f32], metric: DistanceMetric) -> memfuse_core::Result<f32> {
       if a.len() != b.len() {
           return Err(memfuse_core::MemFuseError::invalid_input("Vector dimensions must match"));
       }
       // NaN-Check ENTFERNT — Invariante wird jetzt an den Trust-Boundary-
       // Punkten (Insert, Query-Eingang) sichergestellt, siehe ADR-0XX.
       let dist = match metric { /* ... unverändert ... */ };
       // ...
   }
   ```
   Dokumentiere diese Invariante explizit im Doc-Kommentar von
   `compute_distance()`:
   ```rust
   /// # Invariant
   /// Callers MUST guarantee that `a` and `b` contain no NaN or infinite
   /// values before calling this function. This is enforced at trust
   /// boundaries (vector insert, query entry point) rather than here,
   /// to avoid O(dimension) redundant validation on every pairwise
   /// distance computation in hot search loops (see ADR-0XX).
   ```

4. Füge Regressionstests hinzu, die beweisen, dass die Invarianten an
   den NEUEN Validierungspunkten weiterhin greifen:
   ```rust
   #[tokio::test]
   async fn test_search_rejects_nan_query_vector() {
       let index = /* ... */;
       let result = index.search(&[1.0, f32::NAN, 0.0], 5).await;
       assert!(matches!(result, Err(MemFuseError::InvalidInput(_))));
   }

   #[test]
   fn test_compute_distance_no_longer_validates_nan_directly() {
       // Dokumentiert bewusst das NEUE Verhalten: compute_distance()
       // selbst validiert NICHT mehr — ruft mit NaN auf und zeigt, dass
       // KEIN Err(InvalidInput) mehr zurückkommt (sondern ggf. ein NaN-
       // Ergebnis, was ok ist, da diese Funktion jetzt einen sauberen
       // Input voraussetzt und Aufrufer die Garantie tragen).
   }
   ```

5. WICHTIG: Prüfe alle direkten Testaufrufer von `compute_distance()` mit
   absichtlich fehlerhaften (NaN-haltigen) Vektoren — diese Tests testeten
   bisher das jetzt entfernte Verhalten und müssen zu den neuen
   Validierungspunkten (Schritt 4) migriert werden, NICHT einfach gelöscht.

DEFINITION OF DONE:
- [ ] `compute_distance()` validiert NICHT mehr auf NaN (nur noch
      Dimensions-Check)
- [ ] Insert-Zeit-Validierung (bereits vorhanden) unverändert bestätigt
- [ ] NEUE Query-Zeit-Validierung an allen `search*()`-Einstiegspunkten in
      `hnsw.rs` ergänzt
- [ ] Migrierte Tests beweisen beide Validierungspunkte weiterhin funktionieren
- [ ] Benchmark (optional) zeigt messbaren Geschwindigkeitsgewinn bei
      hochdimensionalen Suchen
- [ ] cargo test --package memfuse-index — PASS
- [ ] WORKING_STATE.md aktualisiert
```

---

### F-09 — Dual Entry-Point im HNSW vereinheitlichen

```markdown
ROLLE / PERSONA:
Du bist ein Principal Rust HNSW-Implementierer mit tiefem Verständnis der
originalen HNSW-Paper-Semantik (Malkov/Yashunin) bezüglich Entry-Point-
Verwaltung bei inkrementellen Inserts, und Erfahrung im sicheren Refactoring
von nebenläufigem, lock-geschütztem Zustand ohne Verhaltensänderung der
Suchqualität.

REPOSITORY: https://github.com/tfufuz1/memfuse
CRATE: memfuse-index

VERIFIZIERTER IST-ZUSTAND:
- `crates/memfuse-index/src/hnsw.rs` hat zwei separate
  `RwLock<Option<usize>>`-Felder: `entry_point` (Zeile 241, persistenter
  Entry-Point aus mmap oder initial gesetzt) und `ram_entry_point`
  (Zeile 242, Entry-Point für neu eingefügte RAM-Nodes).
- Die Suchlogik (`search_layer` und Umgebung, ab Zeile ~1051) prüft BEIDE
  Entry-Points explizit (`entry_point_opt`, dann separat `ram_entry_point`),
  was die Kontrollflusslogik verzweigt und in mindestens 4 Stellen im Code
  dupliziert wird (Zeilen ~1097, ~1122, ~1229-1230).
- Dies ist funktional korrekt (kein akuter Bug), aber strukturell
  fehleranfällig bei künftigen Änderungen (zwei Zustände statt einem
  synchron zu halten).

AUFGABE — Dies ist ein invasives Refactoring. Gehe vorsichtig und
schrittweise vor, mit Tests VOR jeder strukturellen Änderung:

1. Schreibe ZUERST einen umfassenden Charakterisierungstest-Satz, der das
   AKTUELLE Verhalten exakt festhält (falls noch nicht in ausreichendem
   Maß vorhanden):
   ```rust
   #[tokio::test]
   async fn test_search_finds_both_persisted_and_newly_inserted_nodes() {
       // Index mit initial persistierten Knoten (entry_point gesetzt)
       // UND frisch eingefügten RAM-Knoten (ram_entry_point gesetzt)
       // Suche muss BEIDE Kategorien von Knoten finden können
   }
   ```
   Führe diesen Test VOR jeder Änderung aus und bestätige, dass er grün ist
   — er ist dein Sicherheitsnetz für das Refactoring.

2. Analysiere alle Stellen, an denen `entry_point` und `ram_entry_point`
   gesetzt, gelesen oder in der Suchlogik verzweigt werden:
   `grep -n "entry_point\|ram_entry_point" crates/memfuse-index/src/hnsw.rs`

3. Entwirf die Vereinheitlichung: Ein einzelner `RwLock<Option<usize>>`
   `entry_point`, der bei JEDEM erfolgreichen Insert eines Knotens im
   höchsten bisher gesehenen Layer atomar aktualisiert wird (nicht nur bei
   RAM-Inserts) — dies entspricht der Standard-HNSW-Semantik (der
   Entry-Point ist immer der Knoten mit dem höchsten Layer, unabhängig
   davon, ob er aus mmap geladen oder frisch eingefügt wurde).

   ```rust
   // Beim Insert eines neuen Knotens mit Layer L:
   {
       let mut ep_guard = self.inner.entry_point.write();
       let should_update = match *ep_guard {
           None => true,
           Some(current_ep) => {
               let current_layer = /* Layer des aktuellen Entry-Points ermitteln */;
               L > current_layer
           }
       };
       if should_update {
           *ep_guard = Some(new_node_idx);
       }
   }
   ```

4. Entferne das Feld `ram_entry_point` vollständig, sowie alle
   Verzweigungen, die es separat behandeln (Zeilen ~1097, ~1122,
   ~1229-1230 und deren Umgebung).

5. Verifiziere nach jeder Teiländerung, dass der in Schritt 1 geschriebene
   Charakterisierungstest weiterhin grün bleibt.

6. Zusätzliche Tests für die neue, vereinheitlichte Logik:
   ```rust
   #[tokio::test]
   async fn test_entry_point_updates_to_highest_layer_node() {
       // Füge Knoten mit Layer 0, dann Layer 2, dann Layer 1 ein.
       // Entry-Point muss nach allen drei Inserts auf den Layer-2-Knoten zeigen.
   }

   #[tokio::test]
   async fn test_single_entry_point_survives_mmap_reload() {
       // Persistiere Index, lade neu, prüfe dass entry_point korrekt
       // aus mmap-Header übernommen wird UND bei folgenden Inserts
       // korrekt weiter aktualisiert wird (kein "Schatten"-ram_entry_point
       // mehr nötig)
   }
   ```

DEFINITION OF DONE:
- [ ] Charakterisierungstests VOR dem Refactoring geschrieben und grün
- [ ] `ram_entry_point`-Feld vollständig entfernt
- [ ] Einheitliches `entry_point`-Feld mit korrekter
      "höchster Layer gewinnt"-Update-Logik bei jedem Insert
- [ ] Alle Charakterisierungstests bleiben grün nach dem Refactoring
- [ ] Neue Tests für Entry-Point-Konsistenz über mmap-Reload hinweg
- [ ] Keine Regression in bestehender HNSW-Test-Suite
      (cargo test --package memfuse-index — vollständig PASS, insbesondere
      Recall/Determinismus-Tests)
- [ ] WORKING_STATE.md aktualisiert
```

---

### F-10 — `WorkflowState::graph_hash` von `String` zu `[u8; 32]` typisieren

```markdown
ROLLE / PERSONA:
Du bist ein Rust-Typsystem-Purist mit Erfahrung im Ersetzen semantisch
falscher String-Repräsentationen von Binärdaten (Hashes, IDs, Checksummen)
durch korrekte Byte-Array- oder dedizierte Hash-Typen, unter Wahrung von
Serialisierungskompatibilität für bereits persistierte Daten.

REPOSITORY: https://github.com/tfufuz1/memfuse
CRATE: memfuse-core

VERIFIZIERTER IST-ZUSTAND:
- `crates/memfuse-core/src/types/domain.rs`, `struct WorkflowState` (Zeile 22):
  ```rust
  pub struct WorkflowState {
      pub tx: TxId,
      pub graph_hash: String,
  }
  ```
- Dieser Typ wird vermutlich für Checkpoint-/Savepoint-Vergleiche genutzt
  (Kommentar: "Agent memory graph state footprint"). Ein `String` erlaubt
  ungültige Zustände (beliebige Zeichenketten statt gültiger Hex-Digest)
  und ist ~2× größer als nötig (Hex-Encoding verdoppelt die Byte-Größe
  eines 32-Byte-Hashes auf 64 Zeichen).
- WICHTIG: Prüfe VOR der Änderung, ob `WorkflowState` bereits irgendwo
  serialisiert/persistiert wird (z. B. in Checkpoints) — falls ja, ist
  dies eine Breaking-Change-Migration, kein reines internes Refactoring.

AUFGABE:

1. Finde alle Erzeuger und Konsumenten von `WorkflowState.graph_hash`:
   `grep -rn "graph_hash" --include="*.rs" . | grep -v target`
   Bestimme für jede Fundstelle:
   a) Wird der Hash dort als Hex-String erzeugt (z. B. via
      `format!("{:x}", ...)` oder `hex::encode(...)`)?
   b) Wird er persistiert (Checkpoint-Serialisierung) oder nur transient
      im Speicher verglichen?

2. Prüfe, ob bereits ein `blake3::Hash`-Typ oder Wrapper im Codebase
   genutzt wird, den man hier wiederverwenden kann (Konsistenz mit
   bestehenden Hash-Nutzungen wie in `memfuse-store`'s HMAC-Chain):
   `grep -rn "blake3::Hash\b" --include="*.rs" . | grep -v target`

3. Ändere den Typ:
   ```rust
   #[derive(Debug, Clone)]
   pub struct WorkflowState {
       pub tx: TxId,
       /// Agent memory graph state footprint als 32-Byte Blake3-Digest
       /// (vormals `String`-Hex-Repräsentation — siehe ADR-0XX für Migration).
       pub graph_hash: [u8; 32],
   }
   ```
   Falls Serialisierung nötig ist (Checkpoint-Persistenz), stelle sicher,
   dass `serde` mit `[u8; 32]` korrekt umgeht (Serde unterstützt Arrays bis
   32 Elemente nativ ohne zusätzliche Crate — prüfe die Serde-Version im
   Workspace, um sicherzugehen).

4. Passe alle Erzeuger-Stellen an, den Hash direkt als `[u8; 32]`
   (z. B. via `blake3::hash(...).into()` oder `.as_bytes()`) zu erzeugen,
   statt ihn zunächst zu hex-encodieren.

5. Passe alle Konsumenten-Stellen an (Vergleiche, Logging — für Logging
   ggf. `hex::encode(&graph_hash)` NUR zum Ausgabe-Zeitpunkt nutzen, nicht
   als interne Repräsentation).

6. FALLS `WorkflowState` bereits in existierenden Checkpoints persistiert
   wurde (Schritt 1b ergab "ja"): Implementiere eine Migrationsstrategie:
   - Entweder ein `#[serde(deserialize_with = "...")]`, das sowohl das
     alte Hex-String-Format als auch das neue Byte-Array-Format akzeptiert
     (Übergangszeit), oder
   - Dokumentiere explizit als BREAKING CHANGE mit Migrationsanleitung in
     DECISIONS.md, falls Rückwärtskompatibilität nicht praktikabel ist.

7. Tests:
   ```rust
   #[test]
   fn test_workflow_state_graph_hash_is_32_bytes() {
       let state = WorkflowState { tx: TxId::new(1), graph_hash: [0u8; 32] };
       assert_eq!(state.graph_hash.len(), 32);
   }

   #[test]
   fn test_workflow_state_serde_roundtrip() {
       let state = WorkflowState { tx: TxId::new(1), graph_hash: [42u8; 32] };
       let json = serde_json::to_string(&state).unwrap();
       let restored: WorkflowState = serde_json::from_str(&json).unwrap();
       assert_eq!(restored.graph_hash, state.graph_hash);
   }
   ```

DEFINITION OF DONE:
- [ ] `graph_hash` ist `[u8; 32]` statt `String`
- [ ] Alle Erzeuger/Konsumenten angepasst, kein Hex-Encoding mehr als
      interne Repräsentation
- [ ] Falls Persistenz betroffen: Migrationsstrategie implementiert und
      in DECISIONS.md dokumentiert
- [ ] Tests für Größe und Serde-Roundtrip
- [ ] cargo build --workspace — sauber
- [ ] cargo test --package memfuse-core --package memfuse-agent
      (oder wo immer WorkflowState sonst verwendet wird) — PASS
- [ ] WORKING_STATE.md aktualisiert
```

---

### F-11 — CSR `compact()`: Incremental Update statt Full-Rebuild

```markdown
ROLLE / PERSONA:
Du bist ein Principal Rust Graph-Storage-Ingenieur mit Expertise in
CSR-Graph-Datenstrukturen (Compressed Sparse Row) und inkrementellen
Update-Strategien für große, häufig mutierte Graphen (Vorbild: RocksDB-
ähnliche Delta-Compaction-Strategien, aber für Graphstrukturen statt
Key-Value-Paare).

REPOSITORY: https://github.com/tfufuz1/memfuse
CRATE: memfuse-graph

VERIFIZIERTER IST-ZUSTAND:
- `crates/memfuse-graph/src/csr.rs`, `compact()` (Zeile ~191): Iteriert bei
  JEDEM Aufruf `for i in 0..num_nodes` über ALLE Knoten im Graphen, um die
  CSR-Arrays (`offsets`, `targets`, `weights`, `valid_froms`, `valid_tos`)
  komplett neu aufzubauen — inklusive Knoten, die von der aktuellen
  Delta-Runde (`pending_edges`, `tombstoned_edges`) gar nicht betroffen sind.
- Bei N=10.000 Knoten und M=100.000 Kanten ist dies bei jedem Flush teuer,
  selbst wenn nur wenige neue Kanten seit dem letzten `compact()`
  hinzugekommen sind.
- Dies ist eine GROSSE, strukturelle Änderung mit hohem Regressionsrisiko
  (CSR-Korrektheit ist safety-kritisch für Traversal-Algorithmen).
  Gehe inkrementell vor, mit ausführlicher Testabsicherung VOR jeder
  strukturellen Änderung.

AUFGABE:

1. Schreibe ZUERST eine umfassende Charakterisierungs-Test-Suite für das
   AKTUELLE `compact()`-Verhalten (Referenzimplementierung), falls noch
   nicht in ausreichendem Maße vorhanden:
   ```rust
   #[tokio::test]
   async fn test_compact_produces_correct_csr_after_multiple_deltas() {
       // Mehrere Runden: add_edge, add_edge, tombstone, compact(),
       // add_edge, compact() — prüfe nach JEDER compact()-Runde per
       // Traversal, dass alle erwarteten (und nur die erwarteten) Kanten
       // sichtbar sind.
   }
   ```

2. Analysiere GENAU, welche Knoten von einer gegebenen `pending_edges`/
   `tombstoned_edges`-Delta-Runde betroffen sind:
   - Ein Knoten `i` ist "betroffen", wenn `pending_edges.contains_key(&i)`
     ODER `tombstoned_edges` mindestens eine Kante mit Quellknoten `i` enthält.
   - NUR für diese Knoten muss der Offset-Bereich in den CSR-Arrays neu
     aufgebaut werden — alle anderen Knoten behalten exakt ihre bisherigen
     `targets`/`weights`/`valid_froms`/`valid_tos`-Einträge unverändert.

3. Entwirf die inkrementelle Strategie — DIES IST DER KOMPLEXE TEIL:
   CSR-Arrays sind flach und positionsabhängig (`offsets[i]` und
   `offsets[i+1]` definieren den Bereich für Knoten `i` in `targets`).
   Ein rein inkrementelles Update EINES Knotens verschiebt zwangsläufig
   die Indizes ALLER nachfolgenden Knoten in den flachen Arrays. Es gibt
   zwei praktikable Ansätze — wähle einen und begründe die Wahl:

   a) **Zwei-Tier-Ansatz (empfohlen, geringeres Risiko):** Behalte die
      bestehende, unveränderte CSR-Struktur als "Base Tier" bei und
      führe NUR für tatsächlich betroffene Knoten eine kompakte
      Delta-Struktur (z. B. `HashMap<InternalIndex, Vec<EdgePayload>>`,
      die bereits als `pending_edges` existiert!) parallel weiter — ABER
      löse das eigentliche Performance-Problem, indem `compact()` NICHT
      mehr bei jedem Flush das komplette Base-Tier neu aufbaut, sondern
      NUR dann, wenn die Größe von `pending_edges` einen konfigurierbaren
      Schwellenwert überschreitet (ähnlich der bereits vorhandenen
      `rebuild_threshold`-Logik in HNSW). Zwischen solchen "echten"
      Compactions bleibt der Base-Tier unverändert, und Traversal-Code
      liest transparent aus BEIDEM (Base-Tier + Delta), was strukturell
      dem bereits bestehenden Muster in `hnsw.rs` (`entry_point` +
      `ram_entry_point`, siehe F-09) ähnelt — hier aber bewusst BEIBEHALTEN,
      weil es für Delta-Buffering (im Gegensatz zu Entry-Point-Tracking)
      ein etabliertes, sinnvolles Muster ist.

   b) **Vollständiges Incremental-CSR (höheres Risiko, höherer Gewinn):**
      Baue nur den Teil-Bereich der betroffenen Knoten neu auf und
      verschiebe die nachfolgenden Arrays mittels `Vec::splice()` oder
      äquivalenter In-Place-Operationen — technisch aufwendiger,
      vermeidet aber jegliche dauerhafte Zwei-Tier-Leseverzweigung.

   Empfehlung: Beginne mit Ansatz (a), da er das geringste Regressionsrisiko
   hat und das bestehende `pending_edges`/`tombstoned_edges`-Muster bereits
   fast genau diese Struktur hat — die eigentliche Änderung ist dann primär,
   `compact()` SELTENER aufzurufen (schwellenwertbasiert), statt bei jedem
   Flush bedingungslos den vollen Rebuild durchzuführen.

4. Falls Ansatz (a) gewählt wird: Ergänze eine Konfigurationsoption
   analog zu HNSW:
   ```rust
   /// Number of pending (uncompacted) edges that triggers an automatic
   /// full compact(). Lower values keep read-path complexity minimal
   /// (fewer delta entries to merge per traversal) at the cost of more
   /// frequent full rebuilds. Default: 1000.
   pub compact_threshold: usize,
   ```
   Und passe die Aufrufstelle(n) von `compact()` an, nur bei Überschreiten
   dieses Schwellenwerts tatsächlich den vollen Rebuild durchzuführen,
   statt bedingungslos bei jedem Flush.

5. Benchmark vor/nach:
   ```rust
   // benches/csr_compact_bench.rs
   fn bench_compact_with_small_delta_on_large_graph(c: &mut Criterion) {
       // 10.000 Knoten, 100.000 Kanten Basis-Graph, dann NUR 10 neue
       // Kanten hinzufügen und compact() aufrufen — vorher/nachher-Vergleich
   }
   ```

DEFINITION OF DONE:
- [ ] Charakterisierungstests VOR der Änderung geschrieben und grün
- [ ] Gewählter Ansatz (a oder b) begründet im Abschlussbericht dokumentiert
- [ ] `compact_threshold`-Konfiguration ergänzt (falls Ansatz a)
- [ ] Alle Charakterisierungstests bleiben grün
- [ ] Benchmark zeigt messbare Verbesserung bei kleinen Deltas auf großen Graphen
- [ ] cargo test --package memfuse-graph — vollständig PASS (keine
      Traversal-Korrektheits-Regression, dies ist der kritischste Punkt)
- [ ] WORKING_STATE.md aktualisiert
```

---

### F-12 — InvertedIndex Default-Namespace-Kollision beheben

```markdown
ROLLE / PERSONA:
Du bist ein Senior Rust Storage-Namespace-Ingenieur mit Erfahrung im
Aufspüren und Beheben von Präfix-Kollisionen in Key-Value-basierten
Multi-Tenant-Systemen.

REPOSITORY: https://github.com/tfufuz1/memfuse
CRATE: memfuse-text

VERIFIZIERTER IST-ZUSTAND:
- `crates/memfuse-text/src/inverted.rs`, `new_with_language()` (Zeile ~109):
  ```rust
  let prefix = if namespace == "default" {
      b"__txt:default:".to_vec()
  } else {
      format!("__txt:{}:", namespace).into_bytes()
  };
  ```
  BEIDE Zweige erzeugen für `namespace == "default"` denselben Bytestring
  `__txt:default:` — der `if`-Zweig ist funktional redundant zum `else`-Zweig
  bei genau diesem Eingabewert. Das eigentliche Problem: Eine tatsächliche
  Collection, die vom Nutzer explizit `"default"` genannt wird (was in
  `memfuse-db` ein gültiger, sogar der Standard-Collection-Name ist — siehe
  `MemFuse::drop_collection()`, das "default" explizit als Spezialfall
  behandelt), würde denselben Text-Index-Präfix wie der interne
  "kein-Namespace-angegeben"-Fall verwenden. Eine bewusst leere oder
  fehlende Namespace-Angabe UND eine explizite Collection namens "default"
  sind damit im Text-Index nicht mehr unterscheidbar.

AUFGABE:

1. Bestätige das Kollisionsrisiko konkret: Prüfe, wie `new_with_language()`
   von `memfuse-db` aus aufgerufen wird — wird der `namespace`-Parameter
   dort 1:1 mit dem Collection-Namen befüllt?
   `grep -rn "new_with_language\|InvertedIndex::new" crates/memfuse-db/src/`

2. Falls die Kollision real ist (Collection-Name wird direkt als
   `namespace` durchgereicht): Behebe sie durch ein eindeutiges,
   nicht mit einem gültigen Collection-Namen kollidierbares Sentinel für
   den "kein expliziter Namespace"-Fall, z. B. durch Verwendung eines
   Zeichens, das in Collection-Namen laut Validierung
   (`validate_collection_name()` in `memfuse-mcp/src/lib.rs` oder
   äquivalent in `memfuse-db`) nicht erlaubt ist:

   ```rust
   /// Sentinel-Präfix für den impliziten "kein Namespace"-Fall.
   /// Nutzt ein Null-Byte, das in validierten Collection-Namen nicht
   /// vorkommen kann (siehe `validate_collection_name()`), um garantiert
   /// NICHT mit einer echten Collection namens "default" zu kollidieren.
   const IMPLICIT_NAMESPACE_MARKER: &[u8] = b"__txt:\x00implicit-default\x00:";

   let prefix = if namespace.is_empty() {
       IMPLICIT_NAMESPACE_MARKER.to_vec()
   } else {
       format!("__txt:{}:", namespace).into_bytes()
   };
   ```

   WICHTIG: Wähle die exakte Lösung erst NACH Bestätigung von Schritt 1 —
   falls der `namespace`-Parameter in der Praxis NIEMALS mit einem
   nutzerdefinierten Collection-Namen befüllt wird (z. B. weil er
   ausschließlich intern mit einer festen Konstante aufgerufen wird),
   ist das Kollisionsrisiko rein theoretisch, und eine einfachere Lösung
   (den redundanten `if`-Zweig einfach entfernen und dokumentieren, warum
   er nie zu einer echten Kollision führt) ist ausreichend und mit
   weniger Änderungsrisiko verbunden.

3. Falls eine echte Migration nötig ist (bereits persistierte Text-Index-
   Daten unter dem alten `__txt:default:`-Präfix): Dokumentiere den
   Migrationspfad explizit — bestehende Daten unter dem alten Präfix
   dürfen nicht verloren gehen. Ein Lazy-Migration-Ansatz (beim ersten
   Zugriff auf eine Collection den alten Präfix-Bereich erkennen und
   unter dem neuen Präfix umschreiben) ist hier vermutlich der
   pragmatischste Weg, falls Abwärtskompatibilität gefragt ist.

4. Tests:
   ```rust
   #[test]
   fn test_no_namespace_and_explicit_default_collection_use_different_prefixes() {
       let implicit = InvertedIndex::new_with_language(storage.clone(), "", Language::English);
       let explicit_default = InvertedIndex::new_with_language(storage.clone(), "default", Language::English);
       assert_ne!(implicit.prefix(), explicit_default.prefix(),
           "Implicit no-namespace case must not collide with an explicit collection named 'default'");
   }
   ```

DEFINITION OF DONE:
- [ ] Kollisionsrisiko konkret bestätigt oder widerlegt (Schritt 1)
- [ ] Fix implementiert (Sentinel-Marker ODER dokumentierte Nicht-Relevanz)
- [ ] Migrationsstrategie dokumentiert, falls bereits persistierte Daten
      betroffen sein könnten
- [ ] Regressionstest beweist keine Kollision mehr
- [ ] cargo test --package memfuse-text --package memfuse-db — PASS
- [ ] WORKING_STATE.md aktualisiert
```

---

### F-13 — `rebuild_threshold` zu `min_connectivity_ratio` umbenennen

```markdown
ROLLE / PERSONA:
Du bist ein API-Design-Kurator mit Fokus auf selbsterklärende
Konfigurationsnamen in öffentlichen Bibliotheks-APIs.

REPOSITORY: https://github.com/tfufuz1/memfuse
CRATE: memfuse-index

VERIFIZIERTER IST-ZUSTAND:
- `crates/memfuse-index/src/hnsw.rs`: `HNSW_REBUILD_THRESHOLD: f64 = 0.30`
  (30% gelöschte Knoten lösen einen Rebuild aus), aber das tatsächliche
  Config-Feld `rebuild_threshold` (Zeile 78) wird mit
  `1.0 - HNSW_REBUILD_THRESHOLD = 0.70` initialisiert (Zeile 95) und in
  Vergleichen als "Minimum-Konnektivitäts-Score, unterhalb dessen ein
  Rebuild ausgelöst wird" genutzt (`if score < self.config.rebuild_threshold`,
  Zeilen 1352, 1361, 1623, 1784). Der Feldname suggeriert also einen
  "Trigger-Schwellenwert" (naheliegende Fehlinterpretation: "höherer Wert
  = mehr Rebuilds"), tatsächlich ist der Wert aber eine Konnektivitäts-
  UNTERGRENZE (niedrigerer Wert = mehr Toleranz vor Rebuild).
- Dies ist eine PUBLIC-API-Änderung (Feldname in einer Config-Struktur,
  die vermutlich von außerhalb des Crates konstruiert werden kann) — prüfe
  VOR der Umbenennung, ob dies als Breaking Change behandelt werden muss.

AUFGABE:

1. Prüfe die Sichtbarkeit des Feldes und der umschließenden Struktur:
   `grep -n "pub struct.*Config\|pub rebuild_threshold" crates/memfuse-index/src/hnsw.rs`
   Falls die Struktur `#[non_exhaustive]` ist oder ausschließlich über
   einen Builder mit benannten Methoden konstruiert wird (nicht per
   Struct-Literal von außen), ist die Umbenennung risikoärmer.

2. Führe die Umbenennung durch, mit Rückwärtskompatibilität via Deprecated-
   Alias, falls das Feld direkt (Struct-Literal) von außerhalb des Crates
   konstruierbar ist:

   ```rust
   pub struct HnswConfig {
       // ... andere Felder ...

       /// Minimum acceptable connectivity ratio (0.0-1.0). If the graph's
       /// connectivity score falls BELOW this value (i.e. too many nodes
       /// have been tombstoned relative to the total), a full rebuild is
       /// triggered. Defaults to `1.0 - HNSW_REBUILD_THRESHOLD` (0.70),
       /// meaning a rebuild triggers once connectivity drops below 70%.
       ///
       /// Renamed from `rebuild_threshold` for clarity — the old name
       /// suggested "higher = more rebuilds", but the value is actually
       /// a connectivity FLOOR (lower = more tolerance before rebuild).
       pub min_connectivity_ratio: f64,
   }

   impl HnswConfig {
       #[deprecated(since = "0.X.0", note = "Use `min_connectivity_ratio` — same semantics, clearer name")]
       pub fn rebuild_threshold(&self) -> f64 {
           self.min_connectivity_ratio
       }
   }
   ```

   Falls die Struktur ausschließlich intern konstruiert wird (kein
   externer Struct-Literal-Zugriff): Benenne das Feld direkt um, ohne
   Deprecated-Alias-Overhead, und passe alle ~5 internen Nutzungsstellen an.

3. Aktualisiere alle Doc-Kommentare, die den alten Namen referenzieren
   (Zeilen 77-78, 1349).

4. Aktualisiere `docs/TYPE_REGISTRY.md`, falls `HnswConfig`/
   `rebuild_threshold` dort dokumentiert ist.

5. Test (falls Deprecated-Alias-Pfad gewählt):
   ```rust
   #[test]
   #[allow(deprecated)]
   fn test_rebuild_threshold_alias_matches_min_connectivity_ratio() {
       let config = HnswConfig { min_connectivity_ratio: 0.5, ..Default::default() };
       assert_eq!(config.rebuild_threshold(), 0.5);
   }
   ```

DEFINITION OF DONE:
- [ ] Feld umbenannt zu `min_connectivity_ratio` (mit oder ohne
      Deprecated-Alias, je nach Sichtbarkeitsanalyse aus Schritt 1)
- [ ] Alle internen Nutzungsstellen (Zeilen 1352, 1361, 1623, 1784 und
      Umgebung) aktualisiert
- [ ] Doc-Kommentare korrigiert
- [ ] docs/TYPE_REGISTRY.md aktualisiert, falls betroffen
- [ ] cargo build --workspace — sauber (keine neuen Warnings außer
      erwarteten Deprecation-Hinweisen im eigenen Code, falls Alias gewählt)
- [ ] cargo test --package memfuse-index — PASS
- [ ] WORKING_STATE.md aktualisiert
```

---

### F-14 — `unwrap_or_else(|| panic!(...))` in Compaction entfernen

```markdown
ROLLE / PERSONA:
Du bist ein Zero-Panic-Doktrin-Durchsetzer mit Fokus auf Storage-Engine-
Compaction-Pfade, die unter Produktionslast (nicht nur in Tests) laufen.

REPOSITORY: https://github.com/tfufuz1/memfuse
CRATE: memfuse-store

VERIFIZIERTER IST-ZUSTAND:
- `crates/memfuse-store/src/compaction.rs`, Zeile 917:
  `.unwrap_or_else(|| panic!("missing key {}", key))`
  Dies verletzt die im gesamten übrigen Codebase konsequent durchgesetzte
  Zero-Panic-Doktrin (siehe die zahlreichen "Harden memfuse-*"-Commits in
  der Git-Historie) — es ist der letzte bekannte verbliebene Panic-Pfad
  dieser Art in einem Kernmodul.

AUFGABE:

1. Lies den vollständigen Kontext um Zeile 917 in
   `crates/memfuse-store/src/compaction.rs`, um zu verstehen:
   a) In welcher Funktion befindet sich dieser Aufruf?
   b) Gibt diese Funktion bereits `Result<T, MemFuseError>` zurück, oder
      müsste die Signatur geändert werden?
   c) Unter welchen tatsächlichen Laufzeitbedingungen könnte der Key
      wirklich fehlen (Datenkorruption? Race Condition? Logikfehler?) —
      dies bestimmt die passende Fehlervariante.

2. Ersetze den Panic durch propagierte Fehlerbehandlung:
   ```rust
   // VORHER:
   let value = some_map.get(&key).unwrap_or_else(|| panic!("missing key {}", key));

   // NACHHER (falls umschließende Funktion bereits Result zurückgibt):
   let value = some_map.get(&key).ok_or_else(|| {
       MemFuseError::Internal(format!(
           "Compaction invariant violated: expected key '{}' not found in {} — possible data corruption or logic error",
           key, /* Name der Map/Struktur, z.B. "merged SSTable index" */
       ))
   })?;
   ```
   Falls die umschließende Funktion aktuell KEIN `Result` zurückgibt:
   Ändere die Signatur entsprechend und passe alle Aufrufer an
   (`grep -rn "<Funktionsname>(" crates/memfuse-store/src/` um alle
   Aufrufer zu finden).

3. Füge einen Regressionstest hinzu, der den vormals panik-auslösenden
   Zustand gezielt herbeiführt und beweist, dass jetzt ein sauberes
   `Err(...)` statt eines Prozessabsturzes zurückkommt:
   ```rust
   #[tokio::test]
   async fn test_compaction_missing_key_returns_err_not_panic() {
       // Konstruiere den Zustand, der vorher zum Panic geführt hätte
       // (z.B. künstlich präparierte SSTable/Merge-Situation ohne den
       // erwarteten Key)
       let result = /* betroffene Funktion aufrufen */;
       assert!(result.is_err());
       // Explizit KEIN std::panic::catch_unwind nötig — der Test selbst
       // würde bei einem echten Panic fehlschlagen (Test-Runner fängt
       // Panics als Testfehler ab), aber ein expliziter Result::is_err()-
       // Check beweist zusätzlich, dass die Fehlerbehandlung korrekt
       // propagiert statt zu paniken.
   }
   ```

4. Führe eine finale Grep-Verifikation über den GESAMTEN Workspace durch,
   um sicherzustellen, dass dies tatsächlich der letzte verbliebene
   `panic!`-in-`unwrap_or_else`-Fall in Produktionscode war:
   ```bash
   grep -rn "unwrap_or_else(|| panic!\|unwrap_or_else(||panic!" --include="*.rs" . | grep -v target | grep -v "/tests/" | grep -v "#\[cfg(test)\]"
   ```
   Falls weitere Fundstellen auftauchen, die nicht bereits durch andere
   Prompts abgedeckt sind, behandle sie nach demselben Muster und
   dokumentiere sie zusätzlich im Abschlussbericht.

DEFINITION OF DONE:
- [ ] Zeile 917 in compaction.rs gibt Err(...) zurück statt zu paniken
- [ ] Ggf. Signaturänderung der umschließenden Funktion durchgeführt,
      alle Aufrufer angepasst
- [ ] Regressionstest beweist Err statt Panic
- [ ] Finale Grep-Verifikation zeigt keine weiteren
      `unwrap_or_else(|| panic!(...))`-Vorkommen in Produktionscode
- [ ] cargo test --package memfuse-store — PASS
- [ ] WORKING_STATE.md aktualisiert
```

---

### F-15 bis F-18 — Kleinere Restarbeiten (kompakt zusammengefasst)

```markdown
ROLLE / PERSONA:
Du bist ein Senior Rust Engineer, der eine Reihe kleinerer, unabhängiger
Hygiene- und Kalibrierungs-Fixes in einer einzigen fokussierten Sitzung
abarbeitet. Jeder der folgenden vier Punkte ist eigenständig und kann in
beliebiger Reihenfolge bearbeitet werden.

REPOSITORY: https://github.com/tfufuz1/memfuse

=== F-15: BM25-IDF-Floor überprüfen (memfuse-text) ===

VERIFIZIERTER IST-ZUSTAND: `crates/memfuse-text/src/bm25.rs` Zeile 91
nutzt einen IDF-Floor von `1e-6` für sehr häufige Terme (df ≈ n), statt
`0.0` (Term komplett ignorieren) oder eines robusteren Robertson-IDF.

AUFGABE:
1. Lies den vollständigen IDF-Berechnungscode in `bm25.rs` und den
   Kommentar in `inverted.rs` Zeilen 1461-1462, der die Herleitung erklärt.
2. Recherchiere kurz die Standard-BM25-Praxis (Robertson/Spärck Jones):
   Ein IDF-Floor von exakt `0.0` bedeutet, dass extrem häufige Terme
   (Stoppwort-artig) den Score gar nicht mehr beeinflussen — das ist meist
   erwünscht. Ein winziger positiver Floor wie `1e-6` verhindert zwar
   NaN/negative Scores, kann aber bei Rare-Term-Precision-Tests zu
   winzigen, unerwarteten Score-Beiträgen führen.
3. Entscheide (dokumentiere die Entscheidung im Abschlussbericht):
   a) Floor bleibt bei `1e-6` (falls es einen bekannten numerischen Grund
      gibt, z.B. Division-durch-Null-Vermeidung an einer nachgelagerten
      Stelle) — dann ergänze einen erklärenden Kommentar, warum `0.0`
      nicht gewählt wurde.
   b) Floor wird auf `0.0` geändert — dann passe den Code an und
      verifiziere mit einem Test, dass sehr häufige Terme den Gesamt-Score
      eines Dokuments nicht mehr verändern:
      ```rust
      #[test]
      fn test_bm25_extremely_common_term_zero_idf_contribution() {
          // Term, der in praktisch allen Dokumenten vorkommt (df ≈ n)
          // muss nach dem Fix keinen (oder vernachlässigbaren) Beitrag
          // zum Gesamt-BM25-Score liefern.
      }
      ```
4. cargo test --package memfuse-text — PASS.

=== F-16: ScalarQuantizer automatischen Recalibration-Trigger ergänzen (memfuse-index) ===

VERIFIZIERTER IST-ZUSTAND: `quantizer_recalibration_sample_size` existiert
als Config-Parameter in `hnsw.rs`, und `diskann.rs` Zeile 261 loggt bereits
"Quantization drift > 10% detected — ScalarQuantizer recalibration
recommended" — aber es gibt KEINEN automatischen Trigger, der bei diesem
Log-Hinweis tatsächlich eine Recalibration auslöst.

AUFGABE:
1. Finde die Stelle in `diskann.rs`, die den Drift erkennt und den
   Log-Hinweis ausgibt (um Zeile 261).
2. Ergänze eine Config-Option `auto_recalibrate_on_drift: bool` (Default:
   `false`, um bestehendes Verhalten nicht unerwartet zu ändern) in der
   passenden Konfigurationsstruktur.
3. Falls `auto_recalibrate_on_drift == true` UND Drift > 10% erkannt wird:
   Löse tatsächlich eine Recalibration aus (nutze die bestehende
   `quantizer_recalibration_sample_size`-Logik, falls bereits eine
   manuelle Recalibration-Funktion existiert — suche danach:
   `grep -n "fn.*recalibrat" crates/memfuse-index/src/*.rs`).
4. Test:
   ```rust
   #[tokio::test]
   async fn test_auto_recalibration_triggers_on_high_drift() {
       // Config mit auto_recalibrate_on_drift: true
       // Simuliere Vektoren, die deutlich außerhalb der ursprünglichen
       // Trainings-Min/Max-Range liegen (> 10% Drift)
       // Prüfe, dass eine Recalibration tatsächlich stattfand (z.B. via
       // geändertem internem Zustand oder einem Zähler für
       // Recalibration-Aufrufe)
   }
   ```
5. cargo test --package memfuse-index — PASS.

=== F-17: MemTable::get() auf max_by_key statt versions.last() umstellen (memfuse-store) ===

VERIFIZIERTER IST-ZUSTAND: `crates/memfuse-store/src/memtable.rs`,
`fn get()` (Zeile 178) nutzt `versions.last()` (Insertion-Order-Annahme).
Die snapshot-fähige Variante `get_at_seq()` (direkt darunter) ist bereits
korrekt implementiert (binäre Suche nach `seq`). Die Insertion-Order-
Annahme in `get()` ist korrekt UNTER der Invariante, dass `commit_mutex`
alle Commits serialisiert — aber diese Invariante ist implizit und nicht
defensiv abgesichert.

AUFGABE:
1. Ändere `get()` defensiv, um explizit nach der höchsten `seq`-Nummer zu
   suchen statt sich auf Insertion-Order zu verlassen:
   ```rust
   pub fn get(&self, key: &[u8]) -> Option<(Bytes, u64)> {
       let shard_idx = Self::shard_for(key);
       let entries = self.shards[shard_idx].entries.read();
       entries.get(key).and_then(|versions| {
           versions.iter().max_by_key(|(seq, _, _)| *seq)
               .map(|(seq, val, _tx)| (val.clone(), *seq))
       })
   }
   ```
2. Dokumentiere im Code-Kommentar explizit, WARUM dies defensiv ist
   (Schutz gegen zukünftige Multi-WAL-Replay-Szenarien, bei denen Entries
   theoretisch out-of-order in den `versions`-Vec gelangen könnten, auch
   wenn dies unter der aktuellen `commit_mutex`-Serialisierung nicht
   vorkommen sollte).
3. Benchmark: `max_by_key` über eine typischerweise sehr kurze
   `versions`-Liste (meist 1-3 Einträge pro Key) hat vernachlässigbaren
   Overhead gegenüber `.last()` — verifiziere dies kurz, aber erwarte
   keine relevante Performance-Regression.
4. Test:
   ```rust
   #[test]
   fn test_get_returns_highest_seq_even_if_inserted_out_of_order() {
       // Konstruiere künstlich eine versions-Liste, bei der der Eintrag
       // mit der höchsten seq NICHT der letzte im Vec ist (simuliert
       // eine hypothetische Out-of-Order-Situation)
       // get() muss trotzdem den Eintrag mit der höchsten seq liefern
   }
   ```
5. cargo test --package memfuse-store — PASS.

=== F-18: Deutsche Token-Kalibrierung in estimate_tokens() (memfuse-db) ===

VERIFIZIERTER IST-ZUSTAND: `crates/memfuse-db/src/context.rs`,
`estimate_tokens()` (Zeile 169) wurde bereits deutlich verbessert
(BPE-Approximation mit CJK-Erkennung, Code-Block-Multiplikator,
Interpunktions-Behandlung) — es gibt aber KEINE explizite Kalibrierung
für deutsche Komposita (z.B. "Urlaubsantragsprozess"), die von
Subword-Tokenizern typischerweise in mehr Tokens zerlegt werden als
ein einzelnes englisches Wort vergleichbarer Zeichenlänge.

AUFGABE:
1. Lies die vollständige aktuelle Implementierung von `estimate_tokens()`.
2. Ergänze eine heuristische Erkennung "wortartiger, ungewöhnlich langer
   Tokens" (ein starkes Signal für deutsche Komposita, da Englisch kaum
   Wörter > 15-18 Zeichen ohne Bindestrich/Leerzeichen hat):
   ```rust
   // Innerhalb der ASCII-Wort-Zählschleife, nach Ermittlung der Wortlänge:
   if word_len > 14 {
       // Wahrscheinliches Kompositum (Deutsch: "Urlaubsantragsprozess" = 22 Zeichen).
       // Subword-Tokenizer (BPE/WordPiece) zerlegen solche Wörter überproportional
       // stärker als kurze Wörter — kalibriert auf empirische cl100k_base-Beobachtung
       // an deutschen Komposita (~1 Token pro 4-5 Zeichen statt ~1 Token pro Wort).
       tokens += (word_len as f64 / 4.5).ceil();
   } else {
       tokens += 1.3; // bestehende Kurzwort-Heuristik unverändert
   }
   ```
3. WICHTIG: Kalibriere den Divisor (`4.5` ist ein Startwert, kein
   verifizierter Wert) — falls im Repository Zugriff auf einen echten
   Tokenizer (z.B. über eine lokale Ollama-Instanz mit `/api/show` oder
   eine vendored tiktoken-ähnliche Bibliothek) besteht, validiere den
   Divisor empirisch an 10-20 echten deutschen Komposita-Beispielen aus
   `crates/memfuse-text/src/morphology.rs`'s `KMU_DOMAIN_VOCABULARY`
   (Lager-, Urlaubs-, Fertigungs-Begriffe) statt eine reine Schätzung
   zu übernehmen.
4. Tests:
   ```rust
   #[test]
   fn test_estimate_tokens_german_compound_words() {
       let text = "Der Urlaubsantragsprozess erfordert eine Genehmigung durch die Personalabteilung.";
       let estimate = estimate_tokens(text);
       // Beweise, dass die Schätzung für das lange Kompositum höher ausfällt
       // als eine naive Wortzahl-Schätzung (Wortzahl * 1.3) es täte
       let naive_word_count_estimate = (text.split_whitespace().count() as f64 * 1.3).ceil() as usize;
       assert!(estimate > naive_word_count_estimate,
           "German compound calibration should increase estimate above naive baseline");
   }

   #[test]
   fn test_estimate_tokens_short_german_words_unaffected() {
       // Kurze deutsche Wörter (< 14 Zeichen) müssen weiterhin die
       // bestehende 1.3-Faktor-Heuristik nutzen, keine Regression für
       // den Normalfall.
   }
   ```
5. cargo test --package memfuse-db — PASS.

DEFINITION OF DONE (für alle vier Teilaufgaben F-15 bis F-18):
- [ ] F-15: BM25-IDF-Floor-Entscheidung getroffen und dokumentiert/umgesetzt
- [ ] F-16: Auto-Recalibration-Trigger implementiert und schaltbar (Default: aus)
- [ ] F-17: MemTable::get() nutzt defensiv max_by_key
- [ ] F-18: Deutsche Komposita-Kalibrierung in estimate_tokens() ergänzt
- [ ] Alle zugehörigen Tests grün
- [ ] WORKING_STATE.md aktualisiert
```

---

## 4. Niedrig priorisierte Zukunfts-Prompts (P4 — strategische Vision, kein Bugfix)

Die folgenden drei Punkte sind bewusst NICHT als vollständig ausformulierte Detail-Prompts ausgearbeitet, da sie erhebliche architektonische Vorab-Entscheidungen erfordern, die über den Rahmen eines einzelnen Jules-Prompts hinausgehen. Sie sind hier als **Einstiegs-Prompts** formuliert, deren erste Aufgabe die Erarbeitung eines konkreten Umsetzungsplans ist — die eigentliche Implementierung folgt in einer Folge-Session, nachdem der Plan von einem Menschen freigegeben wurde.

### F-19 — Zero-Copy Clone-Reduktion (P-08 vollständig nachholen)

```markdown
ROLLE / PERSONA:
Du bist ein Principal Rust Performance-Engineer, Spezialgebiet Zero-Copy-
Datenflüsse in Hochdurchsatz-Datenbank-Engines (bytes-Crate, Arc-Sharing).

REPOSITORY: https://github.com/tfufuz1/memfuse
CRATES: memfuse-store, memfuse-db, memfuse-core (Trait-Signatur)

VERIFIZIERTER IST-ZUSTAND: Diese Aufgabe wurde in einer vorherigen Session
(P-08) als "Definition of Done" ausgegeben, aber NICHT umgesetzt:
`crates/memfuse-store/src/lsm.rs` hat weiterhin exakt 32 `.clone()`-Aufrufe
(unverändert seit dem letzten Audit), `StorageEngine::scan_prefix()` gibt
weiterhin `Result<Vec<(Vec<u8>, Vec<u8>)>>` zurück (keine `Bytes`-Migration),
und es existiert kein Clone-Reduktions-Benchmark im Repository.

DIES IST EINE GROSSE, POTENZIELL BREAKING-CHANGE-TRÄCHTIGE AUFGABE
(scan_prefix() ist eine pub Trait-Methode). Beginne NICHT direkt mit der
Implementierung, sondern liefere zuerst einen konkreten Umsetzungsplan:

AUFGABE (Planungsphase):

1. Führe die vollständige Clone-Kategorisierung durch (UNVERMEIDBAR /
   ARC-SHARE / BYTES-SLICE) für alle 32 Fundstellen in `lsm.rs`:
   `grep -n '\.clone()' crates/memfuse-store/src/lsm.rs`
   Erstelle eine Tabelle mit Zeilennummer, Kontext, Kategorie und
   geschätztem Aufwand pro Fundstelle.

2. Zähle und liste ALLE Aufrufer von `StorageEngine::scan_prefix()` im
   gesamten Workspace auf (`grep -rn "\.scan_prefix(" --include="*.rs" .`),
   um den tatsächlichen Blast-Radius einer Signaturänderung zu beziffern.

3. Erstelle einen Mikrobenchmark, der den AKTUELLEN Zustand (Vec<u8>-
   basiert) misst, als Baseline für einen späteren Vorher/Nachher-Vergleich
   — auch wenn die eigentliche Bytes-Migration noch nicht erfolgt:
   ```rust
   // benches/clone_reduction_baseline_bench.rs
   fn bench_scan_prefix_current_vec_u8_baseline(c: &mut Criterion) {
       // Misst den aktuellen Vec<u8>-basierten scan_prefix() bei 10.000 Keys
   }
   ```

4. Liefere im Abschlussbericht EINEN konkreten, priorisierten Vorschlag:
   Welche 5-8 der 32 Clone-Stellen haben das beste Aufwand/Nutzen-
   Verhältnis für eine ERSTE, NICHT-breaking Iteration (z.B. rein interne
   Clones, die durch `Arc<[u8]>` ersetzt werden können, ohne die
   `scan_prefix()`-Signatur anzufassen)? Setze NUR diese in dieser Session um.
   Die größere `Bytes`-Migration von `scan_prefix()` selbst (mit
   Breaking-Change-Konsequenzen) wird explizit als SEPARATE, spätere
   Aufgabe empfohlen, die eine bewusste Freigabe-Entscheidung braucht.

5. Setze die in Schritt 4 identifizierten "Quick Win"-Clones tatsächlich um
   und miss den Effekt gegen die Baseline aus Schritt 3.

DEFINITION OF DONE:
- [ ] Vollständige Clone-Kategorisierungstabelle im Abschlussbericht
- [ ] Aufrufer-Analyse von scan_prefix() mit Blast-Radius-Einschätzung
- [ ] Baseline-Benchmark erstellt
- [ ] 5-8 "Quick Win"-Clones (non-breaking) tatsächlich umgesetzt und
      gegen Baseline gemessen
- [ ] Expliziter, NICHT umgesetzter Vorschlag für die größere
      Bytes-Migration von scan_prefix() als Folge-Aufgabe dokumentiert
      (mit Aufwandsschätzung), zur menschlichen Freigabe
- [ ] cargo test --workspace — PASS (keine Regression durch die
      umgesetzten Quick Wins)
- [ ] WORKING_STATE.md aktualisiert
```

### F-20 — Kalibriertes Kaskaden-Routing in memfuse-router

```markdown
ROLLE / PERSONA:
Du bist ein Machine-Learning-Systems-Engineer mit Erfahrung in
Konfidenz-Kalibrierung für Routing-/Eskalations-Entscheidungen in
mehrstufigen LLM-Systemen (Modell-Kaskaden, Small-Language-Model-Routing).

REPOSITORY: https://github.com/tfufuz1/memfuse
CRATE: memfuse-router

VERIFIZIERTER IST-ZUSTAND: `crates/memfuse-router/src/router.rs` aggregiert
`chunk.relevance`-Werte roh und wendet einen statischen `1.2×`-Multiplikator
an, wenn eine Kandidaten-Entity zu einer Domain-Community eines Nutzerprofils
gehört (Zeilen 61, 97, 111) — es gibt keinen Kalibrierungsschritt (z.B.
Platt-Scaling, Isotonic Regression) und kein Konfidenzintervall vor der
eigentlichen Eskalationsentscheidung (wann wird von einem kleineren zu
einem größeren Modell eskaliert).

AUFGABE (Planungsphase — beginne mit einer konkreten Analyse, nicht
direkt mit Code):

1. Lies `router.rs` vollständig und dokumentiere den GESAMTEN aktuellen
   Entscheidungspfad von "Query kommt rein" bis "Eskalationsentscheidung
   getroffen" als Kommentar-Flowchart im Abschlussbericht.

2. Recherchiere/bewerte, welche Kalibrierungsmethode für dieses System
   angemessen ist — die vorhandenen Rohwerte (`chunk.relevance`, RRF-Scores)
   sind KEINE Wahrscheinlichkeiten und haben keine natürliche [0,1]-
   Interpretation. Ein einfacher, praktikabler erster Schritt (statt
   vollem Platt-Scaling, das gelabelte Trainingsdaten bräuchte, die hier
   nicht vorhanden sind) ist eine empirische Score-Normalisierung
   (Min-Max oder Sigmoid) kombiniert mit einem konfigurierbaren
   Konfidenz-Schwellenwert, unterhalb dessen NICHT eskaliert wird (statt
   des aktuellen festen `1.2×`-Boosts).

3. Entwirf `RoutingConfidence`-Typ:
   ```rust
   #[derive(Debug, Clone)]
   pub struct RoutingConfidence {
       /// Normalisierter Score in [0.0, 1.0], NICHT die rohe Relevanz.
       pub calibrated_score: f32,
       /// Ob dieser Score ausreicht, um OHNE Eskalation zu antworten.
       pub confident_enough: bool,
   }
   ```

4. Implementiere eine erste, einfache Kalibrierungsfunktion (Sigmoid mit
   empirisch bestimmten Parametern, oder Min-Max über eine gleitende
   Fenster-Statistik historischer Scores) und ersetze den festen
   `1.2×`-Boost durch eine kalibrierte Score-Anpassung.

5. Tests, die beweisen, dass die Kalibrierung monoton ist (höherer
   Rohscore → höherer oder gleicher kalibrierter Score, nie umgekehrt)
   und dass die Eskalationsentscheidung bei bewusst mehrdeutigen
   Testfällen (Scores nahe der Entscheidungsgrenze) stabil bleibt.

DEFINITION OF DONE:
- [ ] Vollständige Entscheidungspfad-Dokumentation im Abschlussbericht
- [ ] Kalibrierungsmethode gewählt und begründet
- [ ] RoutingConfidence-Typ implementiert
- [ ] Fester 1.2×-Boost durch kalibrierte Logik ersetzt
- [ ] Monotonie-Tests grün
- [ ] cargo test --package memfuse-router — PASS
- [ ] WORKING_STATE.md aktualisiert
```

### F-21 — `ProvenanceRecord` als abfragbares Herkunfts-Objekt (Grundgerüst)

```markdown
ROLLE / PERSONA:
Du bist ein Principal Rust Architekt mit Spezialisierung auf Memory-
Governance und Provenienz-Tracking in LLM-Agenten-Systemen (Vorbild:
VMG-Provenance-Visibility-Primitiv, MemLineage-Muster).

REPOSITORY: https://github.com/tfufuz1/memfuse
CRATE: memfuse-core (neuer Typ), memfuse-db (Integration)

VERIFIZIERTER IST-ZUSTAND: Es existiert KEIN `ProvenanceRecord`-Typ im
Repository. Herkunftsinformation bei LLM-Konsolidierung
(`consolidate_via_llm()` in `memfuse-db`) wird nur als lose
`source_doc_ids: Vec<DocId>`-Feld und ein `"llm_summarized": true`-Metadata-
Flag getragen — kein dediziertes, abfragbares, verkettetes
Herkunftsobjekt (wer hat wann aus welchem WAL-Segment mit welchem
Prompt-Hash konsolidiert).

DIES IST EIN GROSSES, NEUES FEATURE, KEIN BUGFIX. Beginne mit einem
Minimal-Grundgerüst statt der vollen Spezifikation:

AUFGABE (Grundgerüst-Phase):

1. Definiere den minimalen `ProvenanceRecord`-Typ in einer neuen Datei
   `crates/memfuse-core/src/types/provenance.rs`:
   ```rust
   /// Herkunfts-Nachweis für einen einzelnen Memory-Eintrag.
   /// Append-only, wird unter dem Präfix `__provenance:` persistiert.
   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct ProvenanceRecord {
       pub provenance_id: u64,
       pub target_doc_id: DocId,
       pub created_at_tx: TxId,
       /// Art der Operation, die diesen Nachweis erzeugt hat.
       pub operation: ProvenanceOperation,
   }

   #[derive(Debug, Clone, Serialize, Deserialize)]
   #[non_exhaustive]
   pub enum ProvenanceOperation {
       Ingested { source_path: Option<String> },
       LlmConsolidated { source_doc_ids: Vec<DocId>, prompt_hash: [u8; 32] },
       AgentToolOutput { tool_name: String },
   }
   ```
   Halte dich bewusst NUR an dieses Minimalgerüst — die vollständige
   Spezifikation (Kette/Lineage über mehrere Konsolidierungsrunden,
   WAL-Segment-Referenz) ist bewusst NICHT Teil dieser ersten Iteration.

2. Implementiere eine minimale Persistenzfunktion analog zum bestehenden
   Graph-Persistenz-Muster (`__graph:entity:`-Präfix als Vorlage):
   ```rust
   // In memfuse-db, analog zu bestehenden Persistenzpfaden
   async fn persist_provenance(&self, record: &ProvenanceRecord) -> Result<()> {
       let key = format!("__provenance:{}", record.provenance_id);
       // ... via storage.put() persistieren, analog zu Graph-Entities
   }

   async fn get_provenance(&self, doc_id: &DocId) -> Result<Vec<ProvenanceRecord>> {
       // Scan über __provenance:-Präfix, filtere nach target_doc_id
   }
   ```

3. Verdrahte NUR den einfachsten Fall: Erzeuge einen `ProvenanceRecord`
   mit `ProvenanceOperation::Ingested` bei jedem `Collection::insert()`
   (additiv, kein Breaking Change — falls die Provenance-Erzeugung
   fehlschlägt, darf dies NICHT den eigentlichen Insert blockieren,
   sondern nur geloggt werden — Provenienz ist ein Nice-to-have-
   Nebeneffekt, keine harte Voraussetzung für Kernfunktionalität in
   dieser ersten Iteration).

4. Tests:
   ```rust
   #[tokio::test]
   async fn test_insert_creates_ingested_provenance_record() {
       // Nach insert(): get_provenance(doc_id) muss mindestens einen
       // Record mit ProvenanceOperation::Ingested enthalten
   }
   ```

5. Dokumentiere im Abschlussbericht EXPLIZIT, was NICHT Teil dieser
   Iteration ist (LlmConsolidated-Verdrahtung in `consolidate_via_llm()`,
   AgentToolOutput-Verdrahtung in `memfuse-agent`, Lineage-Ketten über
   mehrere Konsolidierungen hinweg, Abfrage-API-Erweiterungen) als
   klar abgegrenzte Folge-Prompts.

DEFINITION OF DONE:
- [ ] ProvenanceRecord + ProvenanceOperation-Typen definiert
- [ ] Persistenzpfad unter __provenance:-Präfix implementiert
- [ ] insert() erzeugt Ingested-Records additiv, ohne Insert-Pfad zu blockieren
- [ ] Test beweist Grundfunktion
- [ ] Explizite Abgrenzung offener Folgearbeiten im Abschlussbericht
- [ ] cargo test --package memfuse-core --package memfuse-db — PASS
- [ ] WORKING_STATE.md aktualisiert
```

---

## 5. Hinweis zur Nutzung dieser Prompts

Jeder Prompt in Abschnitt 3 und 4 ist eigenständig ausführbar und enthält:
- Eine Rollenbeschreibung, die den fachlichen Kontext für Jules setzt
- Den exakt verifizierten Ist-Zustand (mit konkreten Datei-/Zeilenangaben,
  Stand 2026-08-30 — bei Zeitverzug zwischen dieser Analyse und der
  tatsächlichen Jules-Session sollte der Ist-Zustand vor Beginn der
  Implementierung nochmals live per `grep`/Dateiblick verifiziert werden,
  da sich der Code zwischenzeitlich weiterentwickelt haben kann)
- Konkrete, schrittweise Aufgaben mit Code-Gerüsten
- Eine explizite "Definition of Done"-Checkliste

Empfohlene Bearbeitungsreihenfolge: F-01 und F-02 zuerst (kleinster
Aufwand, schließen unmittelbar vorherige Sessions sauber ab), dann F-03
bis F-06 (P1, kleine bis mittlere Inkonsistenzen), dann F-07 bis F-14
(P2, Performance/Hygiene, unabhängig voneinander parallelisierbar), dann
F-15 bis F-18 (P3, kompakte Restarbeiten), zuletzt F-19 bis F-21 (P4,
strategische Vision, jeweils mit eigener Planungsphase vor Implementierung).
