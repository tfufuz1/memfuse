# MemFuse — Erweiterter Implementierungsplan: Status-Abgleich & Ziel-Roadmap

> **Datum der Erststellung**: 2026-09-03  
> **Datum der Aktualisierung**: 2026-09-04  
> **Basis**: Code-Analyse aller 15 Workspace-Crates, DECISIONS.md (ADR-001 bis ADR-056)  
> **Status**: Abgeglichen gegen Codestand vom 2026-09-04  
> **Nächste freie ADR**: ADR-057

---

## 1. Status-Zusammenfassung aller Arbeitspakete

| Arbeitspaket / Bereich | Urspgl. Prio | Aktueller Status | Umsetzung / Beleg |
|---|:---:|---|---|
| **AP-1: HNSW Lock-Free CoW Rebuild** | Prio 1 | 🔴 **Offen & Integrierbar** | In `hnsw.rs:1686` hält `rebuild()` nach wie vor den Schreib-Lock über die gesamte Rebuild-Dauer. Geplant für **ADR-057**. |
| **AP-2: Router Conformal Calibration** | Prio 2 | 🟢 **Erledigt / Veraltet** | **ADR-050 & ADR-054** umgesetzt. Legacy `recalibrate()` wurde entfernt, Single Conformal Calibration & atomarer Read/Write Scope sind im Code active. |
| **AP-3: Compaction OCC & Intent Recovery** | Prio 3 | 🟡 **Teilweise erledigt** | **ADR-051, 052, 053** haben Delete-Propagierung & PinGuard Drop Orphans gelöst. `refresh()` OCC-Retry & Intent Startup-GC sind noch offen. |
| **AP-4: DiskANN Full Production Lifecycle** | Prio 8 | 🔵 **Zurückgestellt** | **ADR-013** (Experimental/Phase 3) bestätigt. DiskANN bleibt als isoliertes Out-of-Core-Feature vorerst hinter Feature-Gate. |
| **AP-5: Agent Dead-Letter & Step-Timeout** | Prio 4 | 🔴 **Offen & Integrierbar** | `memfuse-agent` hat noch keine LSM-Dead-Letter-Persistierung, Step-Timeouts und Budget-Settle. Geplant für **ADR-059**. |
| **AP-6: 4-Signal ProvenanceRecord** | Prio 5 | 🔴 **Offen & Integrierbar** | ProvenanceRecord wird an ~31 Stellen in `fusion.rs`/`search.rs` noch mit `None` befüllt. |
| **AP-7: Layer 2/3 Batch-Pfade** | Prio 6 | 🔴 **Offen & Integrierbar** | `memfuse_batch_insert` MCP-Tool und Agent `batch_persist()` fehlen noch. |
| **AP-8: Cluster Stubs Cleanup** | Prio 7 | 🟢 **Erledigt / Veraltet** | Dead Cluster-Stubs (`init_cluster` etc.) wurden bereits vollständig aus `memfuse-db` entfernt. |
| **SEC-01..07 Befunde** | - | 🟢 **Erledigt / False Positive** | SEC-01 (False Positive), SEC-02 (Flatbuffers Verifier ok), SEC-03 (docx.rs Limits ok), SEC-07 (MCP Encryption ok). |

---

## 2. Detaillierte Bewertung der offenen Punkte

### AP-1: HNSW Copy-on-Write Rebuild (Prio 1, Critical)
- **Problem**: In [`crates/memfuse-index/src/hnsw.rs`](file:///home/freddy/Projekte/memfuse/crates/memfuse-index/src/hnsw.rs#L1686) blockiert `self.write_mutex.lock().await` alle Inserts für 5–30 Sekunden während des Rebuilds.
- **Ziel**: 2-Phase Locking (Snapshot + Async Build ohne Mutex, danach kurzer Write-Lock für Delta-Merge der TxIds > Watermark).
- **Zuweisung**: **ADR-057**.

### AP-3 (Rest): Context Compaction OCC Refresh & Recovery (Prio 2, Major)
- **Problem**: Bei OCC-Konflikten in [`context_compaction.rs`](file:///home/freddy/Projekte/memfuse/crates/memfuse-db/src/context_compaction.rs) schlägt die Session unrecoverable fehl. Verwaiste CommitIntents verbleiben bei Crashes im Storage.
- **Ziel**: `ConsolidationSession::refresh()` für inkrementelles Re-Summarizing + Startup GC verwaister Intent-Keys.
- **Zuweisung**: **ADR-058**.

### AP-5: Agent Dead-Letter Queue & Timeouts (Prio 3, Major)
- **Problem**: In [`crates/memfuse-agent/src/engine.rs`](file:///home/freddy/Projekte/memfuse/crates/memfuse-agent/src/engine.rs) führen fehlerhafte Tool-Calls zum Verlust des Step-Inputs. Tool-Calls haben kein Timeout.
- **Ziel**: LSM-Persistierung von `DeadLetter` (`dead_letter:{task_id}:{step}`), `tokio::time::timeout` für Tool-Calls, 2-Phase Budget (`reserve`/`settle`).
- **Zuweisung**: **ADR-059**.

### AP-6: 4-Signal ProvenanceRecord End-to-End (Prio 4, Feature)
- **Problem**: `SearchResult` enthält an 31 Stellen `provenance: None`.
- **Ziel**: `build_provenance()` in `fusion.rs` befüllt `vector_score`, `text_score`, `graph_score`, `fused_score`, `rerank_score` und `matched_signals` transparent.

### AP-7: Layer 2/3 Batch-Pfade (Prio 5, Performance)
- **Problem**: `insert_many()` ist in Layer 2 da, wird aber von MCP & Agent Orchestrator nicht genutzt.
- **Ziel**: Exponieren des MCP-Tools `memfuse_batch_insert` und Nutzung in `OrchestratorEngine::batch_persist()`.

---

## 3. Aktualisierter Verifikations- & Ausführungsplan

```bash
cargo check --workspace --exclude memfuse-tauri
cargo test --workspace --exclude memfuse-tauri
just check
just dag-check
just debt-audit
just triple-test
```
