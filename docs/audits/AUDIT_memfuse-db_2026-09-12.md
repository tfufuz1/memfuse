# Audit Report: `memfuse-db`

**Crate:** `memfuse-db` (Layer 4 — Orchestrator & 4-Signal-Fusion Engine)
**Datum:** 2026-09-12
**HEAD:** `HEAD 70578ec46e0d1487118dd0aa0479fab7a6d09043, 2026-09-12 00:37:34 +0200`
**Session:** `504d02fc`
**Task-ID:** `JULES-20260911-EIGENB`
**Review Focus:** `nan-and-tie-cases` (RRF Rank Fusion & Numerics)
**Status:** 🟢 Clean / Audited & Verified

---

## 1. Übersicht & Scope

`memfuse-db` ist die zentrale Orchestrierungs- und Fusions-Engine von MemFuse (Layer 4). Sie vereint 4 Suchsignale (HNSW Vektorsuche, BM25 Volltextsuche, CSR-Graph-Traversierung und Metadaten-Filterung) über Weighted Reciprocal Rank Fusion (RRF).

### Module & LOC
- `background_workers.rs`: Background-Reaper Tasks (`start_consolidation_reaper`, `start_expiry_reaper`, `start_thermostat_reaper`, `start_orphan_reaper`)
- `chunker.rs`: `MarkdownChunker` mit überschriftenbasierter Zerlegung
- `collection/crud.rs`: Collection CRUD Operationen (Insert, Upsert, Delete, Get, Scan Bounded)
- `collection/kv_lock.rs`: Feingranulare Key-Locks via 16-Shard Hash
- `collection/maintenance.rs`: Repair, Expiry-Cleanup & Community-Detection
- `collection/mod.rs`: Collection-API, Lock-Hierarchie (`collections` -> `insert_lock` -> `embedder`)
- `collection/query_builder.rs`: HybridQueryBuilder Fluent-API
- `collection/relate.rs`: Zettelkasten-Traversierung über Document Chunk Links
- `collection/search.rs`: `hybrid_search` Orchestrierung aller 4 Signale mit `CheckpointPinGuard`
- `collection/tests.rs`: Integrationstests der Collection-API
- `collection/tx.rs`: Kanonische AtomicU64 TxId-Allokation
- `consolidation_executor.rs`: `ConsolidationEngine` für strukturelle Konsolidierung & Tombstoning
- `context.rs`: `ContextManager` autonomes Kontextfenster-Retriever
- `context_compaction.rs`: `ContextCompactor` & `ConsolidationSession`
- `decay_controller.rs`: `AdaptiveDecayController` für thermodynamisches Adaptive-Decay
- `filter.rs`: Metadaten-Filterung und Memory-Type Extraktion
- `fusion.rs`: `weighted_reciprocal_rank_fusion` Kern der 4-Signal-Fusion & Metadata Merging
- `homeostat.rs`: `RerankPidController` (P95 Latenz-Feedback PID-Regler)
- `lib.rs`: Orchestrator Einstiegspunkt & `MemFuse` Facade
- `maintenance_config.rs`: Konfiguration für Background Maintenance
- `maintenance_scheduler.rs`: Zentraler `MaintenanceScheduler`
- `memory_consolidation.rs`: Structural Consolidation Pass (Near-Duplicate & Clustering)
- `multistep.rs`: `MultiStepEngine` iteratives Query-Rewriting
- `synthesis_phase.rs`: Generative Synthesis Pass
- `temporal_filter.rs`: Bi-temporaler Validity-Filter für Post-RRF-Ergebnisse
- `transaction.rs`: 2PC-Koordination über Sub-Engines
- `volatile_vault.rs`: Ephemerer Vault im RAM mit Zeroize-on-Drop

---

## 2. Inventar-Realitätsabgleich & Drift-Analyse (Schritt 0)

### Inventar-Drift
- **Gefunden:** `background_workers.rs` ist im Repository vorhanden (`crates/memfuse-db/src/background_workers.rs`), war jedoch im Prompter-Inventar vom 2026-09-10 nicht gelistet. `reaper.rs` war im Prompter-Inventar gelistet, ist jedoch im Repository nicht mehr vorhanden.
- **Befund:** `Inventar-Drift: Datei crates/memfuse-db/src/background_workers.rs im Prompter-Inventar vom 2026-09-10 nicht erfasst` und `Inventar-Drift: Datei crates/memfuse-db/src/reaper.rs umbenannt oder entfernt`.
- **Status:** Die Reaper-Hintergrundaufgaben sind vollständig in `crates/memfuse-db/src/background_workers.rs` konsolidiert.

---

## 3. Implementierung & Review-Fokus: RRF Rank Fusion & Numerik (`nan-and-tie-cases`)

### Analysierte Komponenten & Befunde
1. **Zero-Smoothing Boundary (`k=0` Boundary Condition)**
   - **Befund:** In `crates/memfuse-db/src/fusion.rs` forderten debug_asserts `debug_assert!(rrf_k > 0.0)`. Bei Tests mit dem mathematischen Randfall $k = 0$ schlug die Assertion fehl (`rrf_k must be positive`).
   - **Analyse & Fix:** RRF-Ränge in MemFuse sind 1-basiert ($r \ge 1$), wodurch der Nenner $k + r \ge 1.0 > 0$ auch für $k = 0.0$ garantiert positiv und divisionssicher bleibt.
   - **Umsetzung:** In `build_provenance` und `weighted_reciprocal_rank_fusion_with_options` wurden die Assertions zu `debug_assert!(rrf_k >= 0.0)` gelockert und entsprechende implementation markers (`// DONE(memfuse-impl): Allow rrf_k >= 0.0 ... [ref:eigenbau-rrf-fusion]`) gesetzt.

2. **Numerische Stabilität & NaN/Infinity Handling (NC-6 Invariante)**
   - **Verifikation:** `apply_resonance_bonus` und `weighted_reciprocal_rank_fusion_with_options` filtern bzw. partitionieren unendliche und NaN-Scores deterministisch.
   - **Testabdeckung:** `test_nan_score_sorted_to_end_stable_order_preserved` und `test_infinity_score_handling` in `crates/memfuse-db/tests/fusion_edge_cases_test.rs` belegen, dass NaN-Scores ans Ende sortiert werden und keine Score-Korruption verursachen.

3. **Deterministisches Tie-Breaking**
   - **Verifikation:** HeapEntry-Sortierung und Post-RRF Sortierungen nutzen `f32::total_cmp` kombiniert mit sekundärem `then_with(|| a.id.cmp(&b.id))`.
   - **Testabdeckung:** `test_exact_rank_tie_in_both_signals_deterministic_tiebreak` in `fusion_edge_cases_test.rs` verifiziert, dass Ergebnisse bei identischen RRF-Scores unabhängig von der Eingabereihenfolge deterministisch geordnet bleiben.

---

## 4. Test- & Gate-Verifikation

- `cargo test -p memfuse-db --test fusion_edge_cases_test --all-features`: 8/8 Tests PASSED.
- `cargo check --workspace --exclude memfuse-tauri`: 0 Fehler.
- `cargo test -p memfuse-db --all-features`: Alle Tests grün.
