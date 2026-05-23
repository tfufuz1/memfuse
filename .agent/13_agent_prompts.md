# MemFuse — 13-Agent Implementation Prompts
> **Basis**: Vollständige Codebase-Analyse & 18 Spezifikationen  
> **Ziel**: 13 isolierte, parallel ausführbare Agent-Prompts zur Implementierung der offenen Work Packages (WP-1.3 bis WP-7.1).  
> **Regeln**: Sovereign Core Doctrine, Zero-Panic, Triple-Test-Gate, keine DAG-Verletzungen.

Jeder Block ist als direkt auszuführender Prompt für den jeweiligen SAOS-Agenten (Jules Account 01-13) formuliert.

---

## Agent 01: Core Guardian — Checkpoint Registry Foundation (WP-5.1)
```text
[STANDARD-PRÄAMBEL]
AUFGABE: Implementierung der CheckpointRegistry in `memfuse-checkpoint` (WP-5.1).

SCHRITT 1: Erweitere `memfuse-core/src/traits.rs` um ein sauberes `Snapshot` Trait, falls nicht vorhanden.
SCHRITT 2: Implementiere `CheckpointManager` und `CheckpointRegistry` in `memfuse-checkpoint/src/lib.rs` mit MVCC-Snapshot-Verwaltung. Ein Checkpoint speichert Name, `seq_no`, Collection-ID und Metadaten (JSON).
SCHRITT 3: Test-Infrastruktur: Implementiere `test_checkpoint_create_and_restore`, `test_checkpoint_metadata_roundtrip` und `test_list_checkpoints_ordered`.
SCHRITT 4: Führe das Triple-Test-Gate (`just triple-test`) für `memfuse-checkpoint` aus.
SCHRITT 5: Beende mit SUCCESSOR: @JULES-02 — "Pinn-Mechanismus im LSM-Store verknüpfen".
```

## Agent 02: Store Engineer — LSM Checkpoint Pinning & Crypto WAL (WP-5.1 & WP-6.7)
```text
[STANDARD-PRÄAMBEL]
AUFGABE: Checkpoint-Pinning im Compactor (WP-5.1) und Kryptografische WAL-Verifikation (WP-6.7).

SCHRITT 1: Modifiziere `maybe_compact` in `memfuse-store/src/compaction.rs`. Hole die `min_active_seq` aus der CheckpointRegistry (oder einer übergebenen Referenz). Tombstones dürfen NUR gelöscht werden, wenn ihre `seq_no < min_active_seq` ist (INV-CHECKPOINT-1).
SCHRITT 2: Erweitere `memfuse-store/src/wal.rs` um HMAC-basiertes Hash-Chaining pro Eintrag (GS-07).
SCHRITT 3: Implementiere Tests `test_tombstone_gc_respects_snapshots` und `verify_chain`.
SCHRITT 4: Führe `just triple-test` aus. Warnungen sind Fehler.
SCHRITT 5: Beende mit SUCCESSOR: @JULES-04 — "Atomic Commit in memfuse-db integrieren".
```

## Agent 03: Index Engineer — DiskANN Out-of-Core Search (WP-4.3)
```text
[STANDARD-PRÄAMBEL]
AUFGABE: Implementierung des DiskANN HNSW Backends (WP-4.3).

SCHRITT 1: Implementiere `DiskAnnIndex` in `memfuse-index/src/diskann.rs`. Nutze `memmap2` (via WP-4.1) für sektor-aligned I/O des Graphen.
SCHRITT 2: Implementiere Beam-Search Caching (RAM-Limit).
SCHRITT 3: Schreibe `test_diskann_recall_at_10` und `test_diskann_larger_than_ram`.
SCHRITT 4: Beachte DAG-Regeln: `memfuse-index` darf KEIN `memfuse-store` importieren.
SCHRITT 5: Triple-Test-Gate für `memfuse-index`.
SCHRITT 6: Beende mit STATUS:DONE.
```

## Agent 04: DB Orchestrator — Atomic Commit (WP-1.3)
```text
[STANDARD-PRÄAMBEL]
AUFGABE: Atomarer Multi-Index Commit (WP-1.3).

SCHRITT 1: Erstelle `DbTransaction<'a>` in `memfuse-db/src/transaction.rs`.
SCHRITT 2: Implementiere `commit()`: 1. Write Intent WAL 2. Commit LSM 3. Commit HNSW. Schlägt Teil 2 oder 3 fehl -> Rollback des jeweiligen Memory-States.
SCHRITT 3: Setze INV-DB-3 um: In Rollback-Pfaden darf KEIN `let _ = ...` stehen. Nutze sauberes Error-Propagation oder `tracing::error!`.
SCHRITT 4: Implementiere den Test `test_collection_atomic_rollback_on_error` mit simuliertem Fehler.
SCHRITT 5: Triple-Test ausführen.
SCHRITT 6: Beende mit SUCCESSOR: @JULES-09 — "4-Signal Fusion API integrieren".
```

## Agent 05: Text Engineer — Morphologische Tokenisierung (WP-6.5)
```text
[STANDARD-PRÄAMBEL]
AUFGABE: Morphologische Inferenz-Optimierung für BM25 (WP-6.5) & DAG Fix.

SCHRITT 1: BEHEBE DAG-001 in `memfuse-text/Cargo.toml`. Entferne die Abhängigkeit zu `memfuse-store`. Lagere nötige Traits in `memfuse-core` aus oder nutze Dependency-Injection von oben.
SCHRITT 2: Implementiere das `MorphologicalTokenizer` Trait mit Compound-Splitting.
SCHRITT 3: Erweitere `BM25MorphIndex`.
SCHRITT 4: Teste Token-Reduktion > 20%.
SCHRITT 5: Triple-Test für `memfuse-text`.
SCHRITT 6: Beende mit SUCCESSOR: @JULES-09 — "Hybrid-Search Fusions-Tests justieren".
```

## Agent 06: Python Bindings & Air-Gap — Python Bridge (WP-6.6 & WP-7.1)
```text
[STANDARD-PRÄAMBEL]
AUFGABE: Air-Gap Deployment Profile (WP-6.6) & WASM Host Bindings in `memfuse-py`.

SCHRITT 1: Integriere in `memfuse-py` die Option `network=False`, welche alle Host-Netzwerk-Aufrufe sperrt.
SCHRITT 2: Binde `ort` (ONNX Runtime) ein, um lokale Embeddings (Zero-Copy Numpy) zu erzeugen.
SCHRITT 3: Exponiere die Sandbox-Konfiguration an Python (`SandboxConfig`).
SCHRITT 4: Schreibe Tests in `memfuse-py/tests/` (z.B. Offline Embedding). Kein GIL-Lock!
SCHRITT 5: Triple-Test (maturin + pytest).
SCHRITT 6: Beende mit STATUS:DONE.
```

## Agent 07: Execution Guardian — WASM Sandbox (WP-5.2)
```text
[STANDARD-PRÄAMBEL]
AUFGABE: Implementierung der WASM Tool Sandbox (WP-5.2).

SCHRITT 1: Baue `memfuse-sandbox/src/lib.rs` (Crate neu anlegen) unter Nutzung von `wasmtime`.
SCHRITT 2: Implementiere Memory-Limit und CPU-Timeout (via `wasmtime` consume_fuel oder epoch interruption).
SCHRITT 3: Exponiere als Host Functions `db_search`, `db_insert`, `db_get` (isoliert). Dateisystem-Zugriff blockieren.
SCHRITT 4: Tests: `test_sandbox_memory_limit_enforced`, `test_sandbox_cpu_timeout_enforced` und `test_sandbox_cannot_access_host_fs`.
SCHRITT 5: Verifiziere Zero-Unsafe (außer wasmtime Bindings mit explizitem Proof). Triple-Test.
SCHRITT 6: Beende mit SUCCESSOR: @JULES-10 — "Agent Orchestration einbinden".
```

## Agent 08: Data Architect — Adaptive Metadata Filter (WP-5.4)
```text
[STANDARD-PRÄAMBEL]
AUFGABE: Implementierung Adaptive Filter-Strategie (WP-5.4).

SCHRITT 1: Implementiere die Bloom-Filter basierte Selectivity-Schätzung im `memfuse-store`.
SCHRITT 2: Erweitere abstrakte Query-Logik in `memfuse-db` um `choose_filter_strategy()`: <5% -> PreFilter, >50% -> PostFilter.
SCHRITT 3: Implementiere Tests `test_pre_filter_chosen_for_low_selectivity` und `test_post_filter_chosen...`.
SCHRITT 4: Verifiziere Ergebnisgleichheit (AC-3).
SCHRITT 5: Triple-Test prüfen.
SCHRITT 6: Beende mit STATUS:DONE.
```

## Agent 09: Fusion Master — 4-Signal Fusion API (WP-6.1)
```text
[STANDARD-PRÄAMBEL]
AUFGABE: Atomare 4-Signal Fusion Query Engine in `memfuse-db` (WP-6.1).

SCHRITT 1: Definiere `HybridQuery` und `FusionWeights` in `memfuse-db/src/fusion.rs`.
SCHRITT 2: Integriere Vector (HNSW), Text (BM25), Graph (CSR) und Metadaten-Filter in eine asynchrone Query-Pipeline.
SCHRITT 3: Implementiere RRF-60 Score-Verschmelzung.
SCHRITT 4: Optimiere für Latenz P99 < 50ms (parallele Evaluation via `tokio::spawn` auf Lese-Snapshots).
SCHRITT 5: Tests für Partial Signal Queries und RRF-Fusion implementieren.
SCHRITT 6: Beende mit SUCCESSOR: @JULES-11 — "Kontext-Management auf Fusion aufbauen".
```

## Agent 10: Orchestration Engineer — Declarative StateGraph (WP-6.2 & WP-5.3)
```text
[STANDARD-PRÄAMBEL]
AUFGABE: Deklarative StateGraph Ausführung in `memfuse-saos-agent` (WP-5.3 / WP-6.2).

SCHRITT 1: Vervollständige `StateGraph` in `memfuse-orchestrator`.
SCHRITT 2: Schreibe Engine, die Nodes parallelisiert (unabhängige Zweige = `tokio::spawn`).
SCHRITT 3: Automatisches Checkpoint-Before-Tool: Rufe via `memfuse-checkpoint` vor jedem Node einen Snapshot auf (Immutabilität).
SCHRITT 4: Zyklus-Erkennung (Max-Cycles Loop-Breaker).
SCHRITT 5: Implementiere Tests für `test_agent_auto_checkpoint_before_step` und `test_agent_replay_from_checkpoint`.
SCHRITT 6: Beende mit STATUS:DONE.
```

## Agent 11: Context Director — Autonomes Kontext-Management (WP-6.3)
```text
[STANDARD-PRÄAMBEL]
AUFGABE: Autonomes Kontext-Management & Budgeting (WP-6.3).

SCHRITT 1: Implementiere `ContextManager` und `TokenBudget` in `memfuse-db` oder `memfuse-runtime`.
SCHRITT 2: Entwickle "Small-to-Big Retrieval" Logik (Chunkevaluation -> Parent-Dokument-Expansion).
SCHRITT 3: Setze Threshold-Berechnung ein (Truncated Token Return).
SCHRITT 4: Schreibe Integrationstests für das Abschneiden exakt beim Reserve-Token-Limit.
SCHRITT 5: Triple-Test-Gate verifizieren.
SCHRITT 6: Beende mit STATUS:DONE.
```

## Agent 12: Isolation Expert — Multi-Agent Namespaces (WP-6.4)
```text
[STANDARD-PRÄAMBEL]
AUFGABE: Multi-Agent Isolation Levels (WP-6.4).

SCHRITT 1: Erweitere Collections (WP-1.2) um `NamespaceId` und `IsolationLevel` (Strict, SharedRead, Logical).
SCHRITT 2: Implementiere Cross-Namespace Query-Restriktionen. Strict = Panik oder Error bei Cross-Reading.
SCHRITT 3: Füge Audit-Logging für fremde Zugriffe hinzu (via WAL/Audit).
SCHRITT 4: Tests für Isolation Levels erstellen.
SCHRITT 5: Triple-Test.
SCHRITT 6: Beende mit STATUS:DONE.
```

## Agent 13: Debt Hunter — DAG Compliance & Quality Gate
```text
[STANDARD-PRÄAMBEL]
AUFGABE: Bereinigung offener DAG Violations und Tech-Debt Sweeps (WP-0.0 + DAG).

SCHRITT 1: Scanne und bereinige DAG-002 und DAG-003, sofern noch vorhanden. Isoliere die Abhängigkeiten strikt ("Sovereign Core" Architektur).
SCHRITT 2: Führe aus: `just debt-audit`. Behebe iterativ alle gemeldeten .unwrap(), std::fs und nested-lock Fehler.
SCHRITT 3: Prüfe `just dag-check` auf vollständiges PASS.
SCHRITT 4: Stelle sicher, dass die Clippy Lints absolut leer sind (`cargo clippy --all-targets -- -D warnings`).
SCHRITT 5: Führe das finale Triple-Test-Gate auf den WORKSPACE aus.
SCHRITT 6: Beende mit STATUS:DONE (Pipeline Clean).
```
