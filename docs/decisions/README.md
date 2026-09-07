# Architecture Decision Records (ADR)

Diese Übersicht verweist auf alle Architecture Decision Records (ADRs) des Projekts MemFuse.
Gemäß Governance (`CONSTITUTION.md`) ist `docs/decisions/` der alleinige kanonische Speicherort für ADRs.

| ADR-Nr | Titel | Datum | Status |
|---|---|---|---|
| [ADR-001](ADR-001-lsm-tree-für-persistenz.md) | LSM-Tree für Persistenz | - | Final |
| [ADR-002](ADR-002-hnsw-für-vektor-indexierung.md) | HNSW für Vektor-Indexierung | - | Final |
| [ADR-003](ADR-003-rrf-für-hybridisierung.md) | RRF (Reciprocal Rank Fusion) für Hybridisierung | - | Final |
| [ADR-004](ADR-004-sovereign-core.md) | Sovereign Core (Pure Rust Policy) | - | Final |
| [ADR-005](ADR-005-feature-based-scaling.md) | Feature-Based Scaling | - | Final |
| [ADR-006](ADR-006-eigenständige-decisionsmd-statt-inline-in-source-o.md) | Eigenständige DECISIONS.md statt inline in SOURCE_OF_TRUTH.md | - | Final |
| [ADR-007](ADR-007-produktstrategie-lokale-agent-memory-library.md) | Produktstrategie — Lokale Agent-Memory-Library (Richtung C) [TEILWEISE ERSETZT durch ADR-018 bzgl. Vertriebskanal-Priorisierung, 2026-08-24] | - | Final |
| [ADR-008](ADR-008-embedding-backend-onnx-ollama-http.md) | Embedding-Backend — ONNX (memfuse-embed) → Ollama HTTP (memfuse-ollama) | - | Final |
| [ADR-009](ADR-009-crate-memfuse-tauri-als-grundgerüst-für-desktop-ap.md) | Crate `memfuse-tauri` als Grundgerüst für Desktop-App ("MemFuse Brain") | - | Final |
| [ADR-010](ADR-010-mcp-transport-http-rest-stub-stdio-json-rpc-20.md) | MCP-Transport — HTTP-REST-Stub → stdio JSON-RPC 2.0 | - | Final |
| [ADR-011](ADR-011-consolidate-checkpoint-subsystems.md) | Consolidate Checkpoint Subsystems (CheckpointCoordinator Trait) | - | Final |
| [ADR-012](ADR-012-invarianten-spannungsfeld-stdfs-innerhalb-spawn-bl.md) | Invarianten-Spannungsfeld — std::fs innerhalb spawn_blocking vs. Pure Async-I/O | - | Final |
| [ADR-013](ADR-013-diskann-als-experimentelles-feature.md) | DiskANN als experimentelles Feature (memfuse-index) | - | Final |
| [ADR-014](ADR-014-regex-engine-wahl-redos-härtung-für-run-regex-tran.md) | Regex-Engine-Wahl & ReDoS-Härtung für `run_regex_transformation` | - | Final |
| [ADR-015](ADR-015-raii-checkpointguard-integration-konsolidierung-in.md) | RAII CheckpointGuard Integration & Konsolidierung in `memfuse-checkpoint` (AGT-CKPT-001 / AGT-STORE-002) | - | Final |
| [ADR-016](ADR-016-docid-64-bit-blake3-trunkierung-und-kollisionsschu.md) | DocId 64-Bit BLAKE3-Trunkierung und Kollisionsschutz (BEFUND AGT-CORE-002) | - | Final |
| [ADR-017](ADR-017-explicit-authorization-of-unsafe-mmap-in-diskann.md) | Explicit Authorization of `unsafe` Mmap in DiskANN (BEFUND AGT-AUDIT-002) | - | Final |
| [ADR-018](ADR-018-doppelstrategie-pypi-library-und-desktop-app.md) | Doppelstrategie — PyPI-Library UND Desktop-App (Auflösung ADR-007/ADR-009-Konflikt) | - | Final |
| [ADR-019](ADR-019-contextual-retrieval-via-combined-text-owned.md) | Contextual Retrieval via `combined_text_owned()` | - | Final |
| [ADR-020](ADR-020-cognitive-operating-system-als-produktvision.md) | Cognitive Operating System als Produktvision | - | Final |
| [ADR-021](ADR-021-multi-signal-rag-pipeline.md) | Multi-Signal RAG-Pipeline (Contextual → RRF → Reranking) | - | Final |
| [ADR-022](ADR-022-dokumenten-entduplizierung-single-responsibility-p.md) | Dokumenten-Entduplizierung & Single Responsibility Protocol | - | Final |
| [ADR-023](ADR-023-kompensierende-transaktion-für-multi-store-relate.md) | Kompensierende Transaktion für Multi-Store relate() Operations (F-01 / AGT-DB-005) | - | Final |
| [ADR-024](ADR-024-snapshot-isolation-auf-storage--und-text-signale-b.md) | Snapshot-Isolation auf Storage- und Text-Signale beschränkt (Vektor/Graph nicht snapshot-isoliert) | - | Final |
| [ADR-025](ADR-025-memory-importance-score-recency-decay-als-post-pro.md) | Memory Importance Score & Recency-Decay als Post-Processing-Filter (Erweiterung ADR-021 & ADR-024) | - | Final |
| [ADR-026](ADR-026-personalized-pagerank-graph-retrieval.md) | Personalized PageRank (PPR) Graph Retrieval | - | Final |
| [ADR-027](ADR-027-label-propagation-für-community-detection-graphrag.md) | Label Propagation für Community Detection & GraphRAG | - | Final |
| [ADR-028](ADR-028-dezentrales-inline-kontextsystem-sekundengenaue-ze.md) | Dezentrales Inline-Kontextsystem, Sekundengenaue Zeitstempel & Verpflichtendes Mehrfach-Session-Review | - | Final |
| [ADR-029](ADR-029-wal-v3-format-tx-id-hmac-integritätskette.md) | WAL-V3 Format & tx_id HMAC-Integritätskette | - | Final |
| [ADR-030](ADR-030-pre-commit-hook-für-rustfmt-workflow-automatisieru.md) | Pre-Commit-Hook für rustfmt & Workflow-Automatisierung | - | Final |
| [ADR-031](ADR-031-realistic-scale-benchmark-suite-semantische-retrie.md) | Realistic-Scale Benchmark Suite & Semantische Retrieval-Evaluierung | - | Final |
| [ADR-032](ADR-032-async-llm-summarization-provenance-tracking-in-con.md) | Async LLM-Summarization & Provenance Tracking in ContextCompactor (ID: AGT-DB-004) | - | Final |
| [ADR-033](ADR-033-bi-temporale-zeitachsen-im-wissensgraphen.md) | Bi-temporale Zeitachsen (Validitätszeit + Transaktionszeit) im Wissensgraphen (Phase 2 Roadmap) | - | Final |
| [ADR-034](ADR-034-runtime-precondition-assertions-in-öffentlichen-lo.md) | Runtime-Precondition Assertions in öffentlichen Low-Level-Distanzfunktionen (`memfuse-index`) | - | Final |
| [ADR-035](ADR-035-governance-system-härtung-prozessregeln-gegen-wied.md) | Governance-System-Härtung — Prozessregeln gegen wiederkehrende Trait-Default-, Typ-Dopplungs- und Stale-Finding-Fehler | - | Final |
| [ADR-036](ADR-036-unsafe-scope-erweiterung-für-test-only-crypto-anti.md) | unsafe-Scope-Erweiterung für test-only crypto anti_tamper | - | Final |
| [ADR-037](ADR-037-vectorindex-generalisierung-in-collections-v.md) | VectorIndex-Generalisierung in Collection<S, V> | - | Final |
| [ADR-038](ADR-038-zettelkasten-memory-links-supersedes-displacement.md) | Zettelkasten Memory Links (A-MEM) & Supersedes Displacement Logic | - | Final |
| [ADR-039](ADR-039-reqwest-als-workspace-dependency-für-memfuse-route.md) | reqwest als Workspace-Dependency für memfuse-router | - | Final |
| [ADR-040](ADR-040-collectionrs-modularisierung.md) | collection.rs Modularisierung (God Object Auflösung) | - | Final |
| [ADR-041](ADR-041-tombstone-bit-disziplin-in-sequenznummer-berechnun.md) | TOMBSTONE_BIT-Disziplin in Sequenznummer-Berechnungen und rollback_to_tx | - | Final |
| [ADR-042](ADR-042-re-integration-von-memfuse-saos-agent.md) | Re-Integration von `memfuse-saos-agent` | - | Final |
| [ADR-043](ADR-043-aktualisierung-von-last-committed-tx-vor-der-sicht.md) | Aktualisierung von `last_committed_tx` vor der Sichtbarmachung von SSTables in `LsmStorage::flush` | - | Final |
| [ADR-044](ADR-044-mcp-write-authorization-sandbox-policy.md) | MCP Write-Authorization & Sandbox Policy (Default Read-Only) | - | Final |
| [ADR-045](ADR-045-entkopplung-von-memfuse-router-und-memfuse-mcp-dur.md) | Entkopplung von `memfuse-router` und `memfuse-mcp` durch IPC JSON-RPC Typverschiebung | - | Final |
| [ADR-046](ADR-046-wiederherstellung-von-memfuse-agent-aus-dem-archiv.md) | Wiederherstellung von `memfuse-agent` aus dem Archiv | - | Final |
| [ADR-047](ADR-047-simd-implementierungsstrategie-stdarch-vs-portable.md) | SIMD-Implementierungsstrategie — std::arch vs portable_simd (AGT-INDEX-002) | - | Final |
| [ADR-048](ADR-048-wal-legacy-key-feature-gating-downgrade-protection.md) | WAL Legacy-Key Feature-Gating & Downgrade Protection | - | Final |
| [ADR-049](ADR-049-audit-log-append-only-enforcement-via-put-kv-if-ab.md) | Audit-Log Append-Only Enforcement via `put_kv_if_absent` | - | Final |
| [ADR-050](ADR-050-router-single-conformal-calibration-lock-scope-con.md) | Router Single-Conformal Calibration & Lock Scope Consolidation | - | Final |
| [ADR-051](ADR-051-context-compaction-delete-error-propagation.md) | Context Compaction Delete Error Propagation | - | Final |
| [ADR-052](ADR-052-synchronous-pinguard-drop-orphan-registration.md) | Synchronous PinGuard Drop Orphan Registration | - | Final |
| [ADR-053](ADR-053-instance-scoped-orphan-state-in-persistentcheckpoi.md) | Instance-Scoped Orphan State in PersistentCheckpointStore | - | Final |
| [ADR-054](ADR-054-unified-router-scoring-toctou-safe-calibration-sco.md) | Unified Router Scoring & TOCTOU-Safe Calibration Scope | - | Final |
| [ADR-055](ADR-055-wal-legacy-key-fallback-protection.md) | WAL Legacy Key Fallback Protection | - | Final |
| [ADR-056](ADR-056-python-ffi-panic-isolation-via-pyerr-exception-map.md) | Python FFI Panic Isolation via PyErr Exception Mapping | - | Final |
| [ADR-057](ADR-057-lücken-dokumentation.md) | Lücken-Dokumentation (Umnummerierung / Ausgelassen) | - | Final |
| [ADR-058](ADR-058-error-logging-pattern-für-synchrones-orphan-state.md) | Error-Logging-Pattern für synchrones Orphan-State Persistieren in Checkpoint | - | Final |
| [ADR-059](ADR-059-python-ffi-panic-isolation.md) | Python FFI Panic Isolation (ehemals docs/decisions/ADR-048) | - | Final |
| [ADR-060](ADR-060-adr-governance-konsolidierung-auf-decisionsmd-als.md) | ADR-Governance — Konsolidierung auf DECISIONS.md als Einzel-Quelle | - | Final |
| [ADR-061](ADR-061-2-phasen-lock-für-hnsw-rebuild.md) | 2-Phasen-Lock für HNSW Rebuild | - | Final |
| [ADR-062](ADR-062-fault-injection-testsuite-für-wal-v3mvcc.md) | Fault-Injection-Testsuite für WAL V3/MVCC (adaptiert aus chimeraDB SPEC-035) | - | Final |
