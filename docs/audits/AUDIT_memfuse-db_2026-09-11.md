# Audit Report: `memfuse-db`

**Crate:** `memfuse-db` (Layer 4 — Orchestrator & 4-Signal-Fusion Engine)
**Datum:** 2026-09-11
**Session:** `e6ab3646`
**Task-ID:** `JULES-20260911-EIGENB`
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
- **Gefunden:** `background_workers.rs` ist im Repository vorhanden, war jedoch im Prompter-Inventar als `reaper.rs` veraltet bezeichnet.
- **Befund:** `Inventar-Drift: Datei crates/memfuse-db/src/background_workers.rs im Prompter-Inventar nicht direkt erfasst (ehemals reaper.rs)`.
- **Status:** Funktionalität ist vollständig in `crates/memfuse-db/src/background_workers.rs` konsolidiert, `reaper.rs` ist ein deprecated Module-Alias in `lib.rs`.

### Public API Verifikation
- Verifiziert: `MemFuse::open`, `MemFuse::collection`, `Collection::query`, `Collection::hybrid_search`, `reciprocal_rank_fusion`, `weighted_reciprocal_rank_fusion` stimmen mit den Beispielen in `README.md` und `AGENTS.md` überein.
- RRF-Rank-Fusion & Numerik in `fusion.rs`: Bestätigt, dass `fusion_ignores_zero_or_negative_weight`, `test_apply_resonance_bonus_handles_nan_score_deterministically` und `test_heap_entry_nan_score_sorts_to_worst_position` abgedeckt sind und stabil funktionieren.

---

## 3. Audit-Befunde & Behebungen

1. **`audit-M-1` (`fusion.rs`): Metadata Scalar Field Preservation**
   - **Befund:** Tag forderte Überprüfung, ob identische Skalarwerte bei Metadata-Merging in JSON-Arrays umgewandelt werden.
   - **Verifikation:** `merge_metadata` prüft explizit `else if t_val != &s_val`. Identische Skalarwerte werden unverändert als Skalare beibehalten.
   - **Status:** Tag als `[RESOLVED]` markiert.

2. **`audit-M-7` (`search.rs`): Checkpoint PinGuard Safety**
   - **Befund:** Tag forderte sicheres Unpinning über alle Fehlerpfade in `search.rs`.
   - **Verifikation:** `search.rs` nutzt durchgängig `CheckpointPinGuard::new` mit Asynchron-Inkrement und Unpinning in `pin_guard.release().await`.
   - **Status:** Tag als `[RESOLVED]` markiert.

3. **`eigenbau-rrf-fusion` (`fusion.rs`): RRF Rank Fusion & Numerics Hardening**
   - **Befund:** `debug_assert!(rrf_k > 0.0)` blockierte theoretische/experimentelle RRF-Konfigurationen mit `k = 0.0`.
   - **Verifikation:** `debug_assert!(rrf_k >= 0.0)` in `build_provenance` und `weighted_reciprocal_rank_fusion_with_options` aktualisiert.
   - **Status:** Behoben (`// DONE(memfuse-impl): Updated rrf_k assertion to allow k=0 boundary condition [ref:eigenbau-rrf-fusion]`). Unit-Tests in `fusion.rs` und `tests/fusion_edge_cases_test.rs` verifiziert.

4. **Workspace Preflight Fixes:**
   - Behoben: Missing State Write Lock acquisition in `LsmStorage::commit` (`memfuse-store`).
   - Behoben: Missing `persist_sync` calls in `InstanceOrphanRegistry::register_orphan_sync` / `register_checkpoint_sync` (`memfuse-checkpoint`).
   - Behoben: Unsafe `VarBuilder::from_mmaped_safetensors` durch sicheres `VarBuilder::from_buffered_safetensors` in `memfuse-candle`.
   - Behoben: Formatierung von TODO-Kommentaren zur Erfüllung von CI Gate 6 (TODO-Grammatik).

---

## 4. Test- & Gate-Verifikation

- `cargo check -p memfuse-db --all-features`: 0 Fehler.
- `cargo test -p memfuse-db --all-features`: 241/241 Unit-Tests grün, alle Integrationstests grün.
- `cargo check --workspace --exclude memfuse-tauri`: 0 Fehler.
- `cargo run -p xtask -- jules-preflight --fast`: **ALLE GATES BESTANDEN**.
