# MemFuse — Masterplan: Gold Standard LLM-OS
> **Version**: 1.0 — Erstellt: 03. September 2026  
> **Basis**: Code-Audit (2.339 commits, 15 Crates, 47 ADRs, 18 Audit-Berichte)  
> **Entwicklungs-Velocity**: ~19 commits/Tag (bestätigt aus Git-Historie: 05. Mai – 03. Sept 2026)  
> **Ziel**: MemFuse als definitiven Gold Standard für lokale SLM/LLM-Systeme etablieren

---

## 0. Planungsgrundlage & Velocity-Kalibrierung

### Gemessene Entwicklungsgeschwindigkeit
| Zeitraum | Commits | Tage | Commits/Tag | Aufgabentyp |
|---|---|---|---|---|
| Phase 1 (RAG-Fundament) | ~800 | 22.08.–30.08.2026 | ~89/Tag | Neue Features, Kern-Algorithmen |
| Phase 1.5 (Härtung) | ~600 | 31.08.–03.09.2026 | ~150/Tag | Bug-Fixes, Audit-Driven Patches |
| Gesamt | 2.339 | 121 | **19,3/Tag** | Gemischt |

> **Kalibrierungs-Annahme für Planung**: 1 Arbeitstag = 8–12 commits (einzelner Entwickler mit AI-Unterstützung).  
> 1 "komplexes Arbeitspaket" = 3–5 Tage = 24–60 commits.  
> Basis-Aufwandseinheit: **1 Woche = 5 Tage** (Realzeit, nicht Commit-Zeit).

### Projektstand am 03.09.2026
```
✅ Phase 1 (RAG-Fundament):     100% — Vollständig abgeschlossen
🔄 Phase 2 (Cognitive Memory):   70% — Teilimplementiert, kritische Lücken offen
📋 Phase 3 (Selbstorganisierung): 30% — PPR + Community Detection implementiert
📋 Phase 4 (Enterprise):          0% — Noch nicht begonnen
```

---

## 1. Kritische Lücken — Priorisiert & Detailliert

### Lücken-Matrix nach Auswirkung × Aufwand

| # | Lücke | Betroffenes Crate | Schwere | Aufwand | ADR |
|---|---|---|---|---|---|
| L-1 | HNSW Rebuild blockiert Schreibpfad | `memfuse-index` | 🔴 KRITISCH | 4–6 Tage | ADR-048 |
| L-2 | Router-Kalibrierung oszilliert | `memfuse-router` | 🟡 MAJOR | 1 Tag | ADR-049 |
| L-3 | Context Compaction ohne OCC-Retry | `memfuse-db` | 🟡 MAJOR | 3–4 Tage | ADR-050 |
| L-4 | ProvenanceRecord wird nicht befüllt | `memfuse-db`, `memfuse-core` | 🟡 MAJOR | 2–3 Tage | ADR-051 |
| L-5 | Agent Dead-Letter-Queue fehlt | `memfuse-agent` | 🟡 MAJOR | 2–3 Tage | ADR-052 |
| L-6 | Batch-Pfade nicht durchgezogen | `memfuse-db`, `memfuse-mcp` | 🟡 MAJOR | 3–4 Tage | ADR-053 |
| L-7 | DiskANN Production-Lifecycle fehlt | `memfuse-index`, `memfuse-db` | 🟡 MAJOR | 20–30 Tage | ADR-054 |
| L-8 | PyO3 Bindings unvollständig | `memfuse-py` | 🟢 MINOR | 4–5 Tage | — |
| L-9 | Cluster-Stubs (dead code) | `memfuse-db` | 🟢 MINOR | 0,5 Tage | — |

**Gesamtaufwand L-1 bis L-6** (kritisch, ohne DiskANN): ~16–21 Tage = **~3–4 Wochen**  
**Gesamtaufwand L-7** (DiskANN allein): ~20–30 Tage = **~4–6 Wochen**  
**Gesamtaufwand L-8 bis L-9** (minor): ~5 Tage = **~1 Woche**

---

## 2. Vollständige Roadmap (05. September 2026 – 30. Juni 2027)

### Übersicht der Phasen

```
Sept 2026     Okt 2026      Nov 2026      Dez 2026    Jan 2027    Feb 2027    Mrz 2027    Apr–Jun 2027
  |             |              |              |            |           |           |             |
  ├─── Phase 2B: Production Hardening (9 Wochen) ──────────────┤
                                               ├── Phase 3: Cognitive Memory (12 Wochen) ──────────┤
                                                                                    ├── Phase 4: Enterprise (14 Wochen) ──┤
```

---

## 3. Phase 2B — Production Hardening (05.09. – 07.11.2026, 9 Wochen)

**Ziel**: Alle 9 identifizierten Lücken schließen, Produktionstauglichkeit unter Last beweisen  
**Exit-Kriterien** (müssen ALLE erfüllt sein, bevor Phase 3 startet):
- [ ] Hybrid-Search-Latenz ≤ 20 µs @ p95 (gemessen mit Bench-Suite)
- [ ] Latenz-Spikes unter Rebuild: **< 100 ms** (war: 5–30 s)
- [ ] Router-Kalibrierung stabil innerhalb ±2% nach 50 Entscheidungen
- [ ] Alle 9 offenen ADRs (048–055) merged und CI-grün
- [ ] `cargo test --workspace` **0 Failures**
- [ ] Benchmark-Suite: 4-Signal-Fusion mindestens 2× schneller als Mem0 (dokumentiert)
- [ ] PyPI Release `memfuse 0.2.0` veröffentlicht

---

### Sprint 2B-1: Concurrency-Fixes & Korrektheit (Woche 1–3, 05.09.–26.09.2026)

#### Sprint-Ziel
Die drei schwerwiegendsten algorithmischen Fehler beheben, die unter Produktionslast auftreten.

---

#### Arbeitspaket 2B-1-A: HNSW Copy-on-Write Rebuild (ADR-048)
**Dauer**: 4–6 Tage  
**Betroffene Datei**: `crates/memfuse-index/src/hnsw.rs`  
**Zuständigkeit**: Layer 1 (memfuse-index)

**Ist-Zustand (verifiziert, Zeile 1685–1786):**
```rust
pub async fn rebuild(&self) -> Result<()> {
    let _write_lock = self.write_mutex.lock().await;  // ← HÄLT LOCK für gesamte Rebuild-Dauer
    // Phase 1: Snapshot (Zeile 1717–1725)
    let new_index = HnswIndex::try_new(config)?;
    // Phase 2: Build (~100+ Zeilen, dauert 5–30s bei 100k Vektoren)
    // Phase 3: Atomic Swap (Zeile 1786)
}   // ← Lock erst HIER freigegeben. ALLE insert/delete/commit blockiert.
```

**Soll-Zustand (2-Phase Lock):**

```
Phase 1 — Lock-freier Snapshot (neue Implementierung):
  1. Kurzer READ-Lock: active_nodes + deleted_nodes snapshotten
  2. snapshot_watermark = self.last_tx_id.load(Ordering::SeqCst)
  3. READ-Lock freigeben
  4. Neuen Index OHNE Lock aufbauen (eigene HnswIndex-Instanz)
  → Während des Builds: INSERT/DELETE akkumulieren sich im alten Index normal

Phase 2 — Atomarer Swap (kurzer Write-Lock):
  1. Write-Lock acquiren
  2. Delta ermitteln: alle Ops mit committed_tx > snapshot_watermark
  3. Delta in new_index einspielen (wenige Operationen)
  4. Atomic Swap der inneren Datenstrukturen
  5. Write-Lock freigeben
```

**Nicht-offensichtliche Risiken & Mitigationen:**

| Risiko | Mitigation |
|---|---|
| Delta-Merge: Delete-then-Insert für gleiche DocId | `last_tx_id` als Watermark; Delta in Reihenfolge abspielen (Insert nach Delete) |
| Quantizer-Drift: neuer Quantizer nicht für Delta-Vektoren kalibriert | Akzeptabler Trade-off; Delta-Vektoren werden mit existierendem Quantizer encodiert, nicht rekalibriert |
| Mmap-Segment bleibt stabil | Nur RAM-Segment wird getauscht; Mmap-Pfad ist unverändert |
| Rebuild triggert während laufendem Rebuild | `rebuild_in_progress: AtomicBool` Flag; zweiter Trigger wird ignoriert |

**Konkrete Implementierungsschritte:**
1. `AtomicBool rebuild_in_progress` in `HnswIndex`-Struct hinzufügen
2. `take_snapshot_for_rebuild()` als separater Hilfs-Funktion (kurzer Read-Lock)
3. `rebuild_from_snapshot()` als lock-freie Funktion (separate `HnswIndex`-Instanz)
4. `apply_delta()` Funktion: iteriert Delta-Queue, spielt Insert/Delete in new_index ein
5. `atomic_swap()` unter kurzem Write-Lock
6. Bestehende `rebuild()` durch 2-Phase-Implementierung ersetzen
7. Proptest: gleichzeitige Inserts während rebuild dürfen nicht verloren gehen

**Tests:**
```rust
#[tokio::test]
async fn test_rebuild_does_not_block_inserts() { /* concurrent inserts während rebuild */ }
#[tokio::test]  
async fn test_delta_merge_preserves_all_inserts() { /* insert_before_watermark + after_watermark */ }
#[tokio::test]
async fn test_rebuild_idempotent_double_trigger() { /* zweiter rebuild wird ignoriert */ }
```

**Exit-Kriterium**: `cargo bench -p memfuse-index` zeigt max. Spike-Latenz < 50 ms (war > 5.000 ms)

---

#### Arbeitspaket 2B-1-B: Router Conformal Calibration Fix (ADR-049)
**Dauer**: 1 Tag  
**Betroffene Datei**: `crates/memfuse-router/src/router.rs` (Zeile 193–209)  
**Zuständigkeit**: Layer 3 (memfuse-router)

**Ist-Zustand (verifiziert):**
```rust
// router.rs:193–209 — ZWEI Feedback-Loops überschreiben sich
state.recalibrate_conformal(non_conformity);       // Loop 1: Gibbs & Candès
if state.times_selected % 10 == 0 {
    state.recalibrate(0.7);  // ← Loop 2: Legacy-Heuristik, überschreibt Loop 1
}
```

**Soll-Zustand:**
```rust
// Schritt 1: Entferne Zeilen 206–209 komplett
state.recalibrate_conformal(non_conformity);  // Einzige Kalibrierungsquelle

// Schritt 2: Warmup-Schwelle anpassen (profile.rs)
// Vorher: if state.window_total > 10
// Nachher: if state.window_total >= 50
```

**Zusätzlicher Fix: Non-Conformity Score-Berechnung (router.rs:199–202):**
```rust
// Vorher (falsch): Konfidenz-Ratio invertiert
let confidence = best_score / second_best_score;
let non_conformity = (1.0 / confidence).clamp(0.0, 1.0);

// Nachher (korrekt): Margin-basierter Non-Conformity Score
let threshold = state.calibrated_min_score;
let non_conformity = (threshold - best_score).max(0.0) / threshold.max(f32::EPSILON);
```

**Tests:**
```rust
#[test]
fn test_calibration_converges_after_50_samples() { /* nur conformal loop aktiv */ }
#[test]
fn test_non_conformity_score_is_margin_based() { /* kein Konfidenz-Ratio mehr */ }
```

**Deprecation**: `RouterProfile::recalibrate()` mit `#[deprecated(since = "0.2.0", note = "Use recalibrate_conformal()")]` annotieren, in separatem Follow-up-PR entfernen.

---

#### Arbeitspaket 2B-1-C: Context Compaction OCC Retry & Crash-Recovery (ADR-050)
**Dauer**: 3–4 Tage  
**Betroffene Datei**: `crates/memfuse-db/src/context_compaction.rs`  
**Zuständigkeit**: Layer 2 (memfuse-db)

**Ist-Zustand (Lücke 1 — Kein Retry):**
```rust
// context_compaction.rs:341
fn validate_occ(&self) -> Result<()> {
    // Bei OCC-Konflikt: Err(StaleRead) — unrecoverable
    // Aufrufer muss LLM-Summarization (10–20s) vollständig wiederholen
}
```

**Ist-Zustand (Lücke 2 — Crash-Recovery):**
```rust
// start() schreibt CommitIntent::Consolidation ins Storage
// Bei App-Neustart: Intent wird NICHT bereinigt → akkumuliert sich im LSM-Tree
```

**Soll-Zustand:**

```rust
impl ConsolidationSession {
    /// Refresh: Re-liest source_doc TxIds, gibt veränderte Docs zurück
    /// → Aufrufer kann nur für geänderte Docs neu summarisieren
    pub async fn refresh(&mut self) -> Result<Vec<(DocId, ContextChunk)>> {
        let changed = Vec::new();
        for source_doc in &mut self.source_docs {
            let current_tx = self.collection.get_doc_tx(&source_doc.id).await?;
            if current_tx != source_doc.snapshot_tx {
                let new_content = self.collection.get(&source_doc.id).await?;
                source_doc.snapshot_tx = current_tx;
                changed.push((source_doc.id.clone(), new_content));
            }
        }
        self.update_intent_key().await?;
        Ok(changed)
    }
}

/// Externes Retry-Protocol im Aufrufer
async fn consolidate_with_retry(session: ConsolidationSession, max_retries: u32) -> Result<()> {
    let mut delay = Duration::from_millis(1);
    for attempt in 0..max_retries {
        match session.commit().await {
            Ok(_) => return Ok(()),
            Err(MemFuseError::StaleRead) if attempt < max_retries - 1 => {
                let changed = session.refresh().await?;
                if !changed.is_empty() {
                    // Nur geänderte Chunks neu summarisieren (inkrementell)
                    session.update_summaries_for(changed).await?;
                }
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_secs(1));
            }
            Err(e) => return Err(e),
        }
    }
    Err(MemFuseError::ConsolidationFailed("Max retries exceeded".into()))
}
```

**Crash-Recovery beim Startup:**
```rust
// memfuse-db/src/lib.rs (in repair_on_open)
async fn cleanup_orphaned_consolidation_intents(storage: &LsmStorage) -> Result<()> {
    let prefix = b"__consolidation_intent:";
    let orphaned = storage.scan_prefix(prefix).await?;
    for (key, _) in orphaned {
        storage.delete(&key).await?; // Tombstone, Garbage Collection
    }
    Ok(())
}
```

**Tests:**
```rust
#[tokio::test]
async fn test_consolidation_retry_on_occ_conflict() { /* stale read → retry → success */ }
#[tokio::test]
async fn test_orphaned_intent_cleanup_on_startup() { /* crash → restart → intents bereinigt */ }
#[tokio::test]
async fn test_incremental_re_summarization() { /* nur geänderte chunks neu zusammengefasst */ }
```

---

### Sprint 2B-2: Feature-Completion & Audit-Kette (Woche 3–6, 19.09.–17.10.2026)

#### Sprint-Ziel
Alle Feature-Lücken schließen: ProvenanceRecord vollständig befüllen, Agent robustifizieren, Batch-Pfade durchziehen.

---

#### Arbeitspaket 2B-2-A: ProvenanceRecord End-to-End Befüllung (ADR-051)
**Dauer**: 2–3 Tage  
**Betroffene Dateien**: `crates/memfuse-db/src/fusion.rs`, `crates/memfuse-db/src/collection/search.rs`  
**Zuständigkeit**: Layer 2 (memfuse-db)

**Ist-Zustand (verifiziert):**
```rust
// fusion.rs — 31× provenance: None im gesamten Codebase
// SearchResult::provenance ist immer None
// SearchResult::matched_signals ist immer Vec::new()
```

**Soll-Zustand:**

```rust
// Neue Hilfs-Funktion in fusion.rs
fn build_provenance(
    vector_rank: Option<usize>,
    vector_score: Option<f32>,
    text_rank: Option<usize>,
    text_score: Option<f32>,
    graph_rank: Option<usize>,
    graph_score: Option<f32>,
    fused_score: f32,
    rerank_score: Option<f32>,
) -> ProvenanceRecord {
    ProvenanceRecord {
        vector_score,
        text_score,
        graph_score,
        fused_score,
        rerank_score,
        signal_count: [vector_score, text_score, graph_score]
            .iter().filter(|s| s.is_some()).count() as u8,
        retrieval_timestamp: SystemTime::now(),
    }
}

// In reciprocal_rank_fusion() — Rang-Informationen WÄHREND der Fusion tracken
pub fn weighted_reciprocal_rank_fusion(
    ranked_lists: &[(Vec<(DocId, f32)>, f32)],  // (results, weight)
) -> Vec<(DocId, f32, ProvenanceRecord)> {      // ← Neu: ProvenanceRecord im Return
    let mut per_doc_signals: HashMap<DocId, PerDocSignals> = HashMap::new();
    
    for (signal_idx, (list, weight)) in ranked_lists.iter().enumerate() {
        for (rank, (doc_id, score)) in list.iter().enumerate() {
            let entry = per_doc_signals.entry(doc_id.clone()).or_default();
            entry.record_signal(signal_idx, rank, *score, *weight);
        }
    }
    
    per_doc_signals.into_iter()
        .map(|(doc_id, signals)| {
            let fused_score = signals.compute_rrf_score();
            let provenance = signals.into_provenance(fused_score);
            (doc_id, fused_score, provenance)
        })
        .collect()
}
```

**API-Erweiterung (memfuse-db/src/collection/search.rs):**
```rust
/// Neue Methode: Search mit vollständiger Audit-Kette
pub async fn search_with_provenance(
    &self,
    query: &str,
    query_embedding: &[f32],
    top_k: usize,
) -> Result<Vec<(SearchResult, ProvenanceRecord)>> { /* ... */ }
```

**`matched_signals` befüllen:**
```rust
// In SearchResult: basierend auf welche Signale einen Score > 0 geliefert haben
result.matched_signals = provenance
    .active_signals()  // ["vector", "text", "graph"] je nach Score > 0
    .collect();
```

**Tests:**
```rust
#[tokio::test]
async fn test_provenance_vector_score_populated() { /* nach hybrid_search sind scores gesetzt */ }
#[tokio::test]
async fn test_provenance_all_signals_populated() { /* alle 4 Signale tracken */ }
#[tokio::test]
async fn test_matched_signals_correct() { /* "vector", "text" wenn graph keinen Hit */ }
```

---

#### Arbeitspaket 2B-2-B: Agent Orchestrator — Dead-Letter, Timeout, Budget (ADR-052)
**Dauer**: 2–3 Tage  
**Betroffene Dateien**: `crates/memfuse-agent/src/engine.rs`, `crates/memfuse-agent/src/audit.rs`  
**Zuständigkeit**: Layer 3 (memfuse-agent)

**Ist-Zustand (3 Lücken):**
1. Fehlgeschlagene Steps gehen verloren (kein Persist, kein Retry)
2. `tool.execute()` hat kein Timeout — hängender Tool-Call blockiert forever
3. `estimated_cost` vs. `tokens_consumed` driften auseinander (Budget-Überbuchung möglich)

**Soll-Zustand:**

```rust
// Neue Struct: DeadLetter Persistenz
#[derive(Serialize, Deserialize)]
pub struct DeadLetter {
    pub step_id: String,
    pub node_id: String,
    pub input: serde_json::Value,
    pub error: String,
    pub timestamp_tx: TxId,
    pub retry_count: u32,
    pub last_attempt: SystemTime,
}

// Erweiterung AgentTool-Trait: timeout() Methode
#[async_trait]
pub trait AgentTool: Send + Sync {
    async fn execute(&self, ctx: &AgentContext, input: serde_json::Value) -> Result<StepResult>;
    fn timeout(&self) -> Duration { Duration::from_secs(60) }  // Default: 60s
    fn max_retries(&self) -> u32 { 0 }  // Default: kein Retry
}

// run_internal() mit Timeout + Dead-Letter:
async fn run_internal(&mut self, tool: &dyn AgentTool, input: serde_json::Value) -> Result<StepResult> {
    let timeout = tool.timeout();
    match tokio::time::timeout(timeout, tool.execute(&self.ctx, input.clone())).await {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(e)) => {
            // Step fehlgeschlagen: Dead-Letter persistieren
            self.persist_dead_letter(DeadLetter {
                step_id: self.ctx.current_step_id.clone(),
                input,
                error: e.to_string(),
                retry_count: 0,
                ..
            }).await?;
            Err(e)
        }
        Err(_timeout) => {
            Err(MemFuseError::Timeout(format!("Tool timed out after {:?}", timeout)))
        }
    }
}

// Dead-Letter Persistenz im LSM-Store
async fn persist_dead_letter(&self, dl: DeadLetter) -> Result<()> {
    let key = format!("__dlq:{}:{}", dl.step_id, dl.timestamp_tx);
    self.storage.put(key.as_bytes(), &bincode::serialize(&dl)?).await
}
```

**2-Phase Budget:**
```rust
// Vorher: reserve_cost + consume_actual in einer Operation
// Nachher: 
let reservation = self.ctx.budget.try_reserve(estimated_cost)?; // Phase 1
let result = tool.execute(&self.ctx, input).await?;
reservation.settle(result.tokens_consumed)?; // Phase 2: überschuss freigeben oder Mehrkosten buchen
```

**Tests:**
```rust
#[tokio::test]
async fn test_failed_step_persisted_to_dlq() { /* after error: dead letter in storage */ }
#[tokio::test]
async fn test_tool_timeout_returns_error() { /* nach 60s: Err(Timeout) */ }
#[tokio::test]
async fn test_budget_settle_handles_overrun() { /* actual > estimated → kein Panic */ }
```

---

#### Arbeitspaket 2B-2-C: Batch-Pfade Layer 2/3 Durchziehen (ADR-053)
**Dauer**: 3–4 Tage  
**Betroffene Dateien**: `memfuse-db/src/collection/crud.rs`, `memfuse-mcp/src/server.rs`, `memfuse-agent/src/engine.rs`  
**Zuständigkeit**: Layer 2 + Layer 3 + Layer 4

**Ist-Zustand:**
- `insert_many()` in `memfuse-db` existiert und hält Single-Lock korrekt
- Aber: MCP hat kein `memfuse_batch_insert` Tool
- Aber: OrchestratorEngine nutzt nur Einzel-Inserts pro Step
- Aber: ContextCompactor verarbeitet Chunks einzeln

**Soll-Zustand (3 Teilaufgaben):**

**Teilaufgabe 1: `memfuse_batch_insert` MCP-Tool**
```rust
// memfuse-mcp/src/server.rs — neues Tool
Tool {
    name: "memfuse_batch_insert",
    description: "Insert multiple documents in one atomic operation",
    input_schema: json!({
        "type": "object",
        "properties": {
            "collection": { "type": "string" },
            "documents": {
                "type": "array",
                "items": { /* doc_id + content + metadata */ }
            }
        }
    })
}

// Handler: delegiert an collection.insert_many()
```

**Teilaufgabe 2: OrchestratorEngine Batch-Persist**
```rust
// engine.rs: Sammle Step-Outputs, schreibe als Batch
pub async fn batch_persist(&self, outputs: Vec<(StepId, StepResult)>) -> Result<()> {
    let docs = outputs.into_iter().map(|(step_id, result)| {
        (format!("{step_id}"), result.embedding, Some(result.metadata))
    }).collect::<Vec<_>>();
    self.collection.insert_many(&docs).await
}
```

**Teilaufgabe 3: ContextCompactor compact_batch()**
```rust
// context_compaction.rs: Verarbeite alle Chunks in einem RW-Zyklus
pub async fn compact_batch(&self, chunks: Vec<ChunkId>) -> Result<()> {
    let guard = self.collection.insert_lock.lock().await;
    let mut db_tx = self.collection.begin_transaction().await?;
    for chunk_id in chunks {
        db_tx.delete_op(&chunk_id).await?;
    }
    db_tx.insert_op(&compacted_id, &compacted_content).await?;
    db_tx.commit().await
}
```

**Tests:**
```rust
#[tokio::test]
async fn test_batch_insert_throughput() { /* 29× schneller als N einzelne inserts */ }
#[tokio::test]
async fn test_mcp_batch_insert_tool_exists() { /* JSON-Schema validiert */ }
```

---

### Sprint 2B-3: DiskANN Production-Lifecycle (Woche 4–9, 26.09.–07.11.2026)

> ⚠️ **Eigenständiges Unterprojekt** — kann parallel zu Sprint 2B-2 beginnen, braucht aber eigenes Feature-Branch.

#### Arbeitspaket 2B-3: DiskANN Full Lifecycle (ADR-054)
**Dauer**: 20–30 Tage  
**Betroffene Dateien**: `crates/memfuse-index/src/diskann.rs`, `crates/memfuse-db/src/collection/`  
**Basis**: ADR-013 (experimentell), ADR-037 (`Collection<S, V: VectorIndex>` generisch)

**Komponenten (4 Teilaufgaben):**

**Teilaufgabe A: Build-Pipeline (HNSW → DiskANN Konversion)**
```rust
// Neues Struct: DiskAnnBuilder
pub struct DiskAnnBuilder {
    sector_size: usize,
    max_degree: usize,
    search_list_size: usize,
    output_path: PathBuf,
}

impl DiskAnnBuilder {
    /// Konvertiert bestehenden HNSW-Index in DiskANN-Mmap-Format
    pub async fn build_from_hnsw(
        &self,
        hnsw: &HnswIndex,
        output: &Path,
    ) -> Result<DiskAnnIndex> {
        // 1. Snapshot aller HNSW-Vektoren (read-lock, kurz)
        // 2. Greedy-Construction Graph (deterministisch, Single-Pass)
        // 3. Sektor-aligniertes Schreiben ins Mmap-File
        // 4. Header + Magic-Bytes schreiben (Integrity Check)
        // 5. DiskAnnIndex aus Mmap-File öffnen und zurückgeben
    }
}
```

**Trigger-Logik: HNSW → DiskANN Migration:**
```
Wenn num_vectors(HNSW) > 500_000: automatisch DiskANN-Build triggern
Alle 1.000.000 neue Inserts: DiskANN rebuild (Hintergrundprozess)
Nach rebuild: HNSW als Delta-Buffer beibehalten
```

**Teilaufgabe B: HybridVectorIndex (HNSW Delta + DiskANN Basis)**
```rust
/// Wrapper für kombinierte HNSW + DiskANN Suche
pub struct HybridVectorIndex {
    primary: HnswIndex,      // Delta-Buffer (aktuell, in-memory)
    secondary: DiskAnnIndex, // Historischer Bestand (Mmap, out-of-core)
}

impl VectorIndex for HybridVectorIndex {
    async fn search(&self, query: &[f32], top_k: usize) -> Result<Vec<(DocId, f32)>> {
        // 1. Suche in beiden Indizes parallel (tokio::join!)
        // 2. Merge via Score-Fusion (kein RRF nötig — gleiche Score-Skala)
        // 3. Deduplizierung (DocId-Set)
        // 4. Top-K Rückgabe
    }
    
    async fn insert(&self, doc_id: DocId, vector: &[f32]) -> Result<()> {
        // Nur in primary (HNSW) schreiben
        self.primary.insert(doc_id, vector).await
    }
    
    async fn delete(&self, doc_id: DocId) -> Result<()> {
        // In beiden löschen: HNSW (remove) + DiskANN (Tombstone-Bitmap)
        tokio::try_join!(
            self.primary.delete(doc_id.clone()),
            self.secondary.mark_deleted(doc_id)
        )?;
        Ok(())
    }
}
```

**Teilaufgabe C: Mmap Warmup & Fault Tolerance**
```rust
// Beim DiskAnnIndex::load():
fn load(path: &Path) -> Result<DiskAnnIndex> {
    // 1. Magic-Bytes validieren (Header-Integrity-Check)
    let mmap = unsafe { Mmap::map(&file)? };
    if &mmap[0..8] != DISKANN_MAGIC_BYTES {
        return Err(MemFuseError::CorruptIndex("DiskANN header invalid".into()));
    }
    // 2. madvise(MADV_SEQUENTIAL) für initiales Warmup
    // 3. SIGBUS-Handler (via sigaction): bei Mmap-Fehler → Err(MmapFault)
    // 4. Graceful Degradation: Err(MmapFault) → auf HNSW-only fallen
    Ok(DiskAnnIndex { mmap, .. })
}
```

**Teilaufgabe D: Collection Integration**
```rust
// memfuse-db/src/lib.rs:
// Neue Fabrik-Methode für billion-scale Collections:
pub async fn collection_billion_scale(&self, name: &str) -> Result<Collection<LsmStorage, HybridVectorIndex>> {
    let hnsw = HnswIndex::try_new(self.hnsw_config())?;
    let diskann_path = self.db_path.join(format!("diskann/{name}.diskann"));
    let diskann = if diskann_path.exists() {
        Some(DiskAnnIndex::load(&diskann_path)?)
    } else {
        None
    };
    let hybrid = HybridVectorIndex::new(hnsw, diskann);
    Collection::open_with_index(self.storage.clone(), hybrid).await
}
```

**Tests:**
```rust
#[tokio::test]
async fn test_diskann_build_from_hnsw() { /* 10k Vektoren → diskann file → query */ }
#[tokio::test]
async fn test_hybrid_index_search_consistency() { /* HNSW-only vs. Hybrid: gleiche Top-10 */ }
#[tokio::test]
async fn test_diskann_graceful_degradation_on_corrupt() { /* korrupte .diskann → HNSW fallback */ }
#[tokio::test]
async fn test_hybrid_delete_removes_from_both() { /* delete in HNSW und DiskANN-Bitmap */ }
```

---

### Sprint 2B-4: Cleanup & Release-Vorbereitung (Woche 8–9, 24.10.–07.11.2026)

#### Arbeitspaket 2B-4-A: Cluster-Stubs entfernen
**Dauer**: 0,5 Tage  
**Betroffene Datei**: `crates/memfuse-db/src/lib.rs` (Zeilen 1055–1096)

```rust
// Zu entfernen: alle #[cfg(feature = "cluster")] Methoden
// init_cluster(), add_peer(), remove_peer(), cluster_status(), step_down()
// Begründung: feature "cluster" existiert nicht in Cargo.toml → dead code
```

#### Arbeitspaket 2B-4-B: PyO3 Bindings erweitern
**Dauer**: 4–5 Tage  
**Betroffene Datei**: `crates/memfuse-py/src/lib.rs`

**Aktuell exponierte API (unvollständig):**
- `search(query, k)` ✅
- `insert(doc_id, embedding, metadata)` ✅

**Fehlende Exports:**
```rust
#[pymodule]
fn memfuse(_py: Python, m: &PyModule) -> PyResult<()> {
    // Neu hinzufügen:
    m.add_function(wrap_pyfunction!(create_collection, m)?)?;
    m.add_function(wrap_pyfunction!(drop_collection, m)?)?;
    m.add_function(wrap_pyfunction!(list_collections, m)?)?;
    m.add_function(wrap_pyfunction!(batch_insert, m)?)?;
    m.add_function(wrap_pyfunction!(search_with_provenance, m)?)?;
    m.add_function(wrap_pyfunction!(relate, m)?)?;
    m.add_function(wrap_pyfunction!(hybrid_search_advanced, m)?)?;
    Ok(())
}
```

**Python-Docstrings + Type-Hints:** Vollständige `.pyi` Stub-Dateien generieren.

#### Arbeitspaket 2B-4-C: PyPI Release 0.2.0 & crates.io Release
**Dauer**: 1–2 Tage

```
Schritte:
1. CHANGELOG.md für 0.2.0 schreiben (alle Phase-2B-Changes)
2. Version in Cargo.toml von 0.1.0 → 0.2.0 bumpen
3. PyO3 maturin build: memfuse-0.2.0-cp311-cp311-*.whl
4. cargo publish -p memfuse-db (crates.io)
5. pip install memfuse via PyPI
6. GitHub Release + Changelog
```

#### Arbeitspaket 2B-4-D: Competitive Benchmark Suite
**Dauer**: 3–4 Tage

```rust
// benches/competitive_benchmark.rs
// Vergleicht MemFuse gegen Mem0 (Python), Chroma (Python), Qdrant (Rust)
// Metriken:
//   - Insert-Latenz: single + batch (100, 1000, 10000 docs)
//   - Search-Latenz: 4-Signal vs. Vector-only
//   - Recall@10: mit/ohne Contextual Retrieval
//   - Memory-Footprint: RSS während Benchmark
//   - Startup-Zeit: cold-start + warm-start

criterion_group!(
    benches,
    bench_insert_single,
    bench_insert_batch_100,
    bench_insert_batch_10000,
    bench_hybrid_search_vs_vector_only,
    bench_recall_at_10_contextual_vs_plain,
    bench_startup_time_cold,
);
```

---

## 4. Phase 3 — Cognitive Memory & Advanced Retrieval (08.11.2026 – 06.02.2027, 12 Wochen)

**Ziel**: MemFuse als echtes "Cognitive Operating System" positionieren — Gedächtnis das denkt, vergisst und lernt.

**Voraussetzung**: Phase 2B vollständig abgeschlossen und Release 0.2.0 veröffentlicht.

---

### Sprint 3-1: Memory Consolidation & Sleep-Cycle (Woche 1–4, 08.11.–05.12.2026)

#### Arbeitspaket 3-1-A: Async Sleep-Cycle Konsolidierung (ADR-055)
**Dauer**: 6–8 Tage  
**Betroffene Crates**: `memfuse-agent`, `memfuse-db`, `memfuse-ollama`

**Konzept**: Periodische Hintergrundaufgabe, die episodische Memories zu semantischen Memories konsolidiert (analog zum menschlichen Schlaf-Gedächtnis-Konsolidierungsprozess).

```rust
/// Sleep-Cycle Konsolidierungsschleife
/// Läuft als Hintergrundtask in memfuse-agent
pub struct SleepCycleConsolidator {
    collection: Arc<Collection<LsmStorage, HnswIndex>>,
    llm: Arc<OllamaClient>,
    cycle_interval: Duration,  // Default: 4 Stunden
    min_memories_threshold: usize,  // Min. Anzahl Episodics zum Konsolidieren
}

impl SleepCycleConsolidator {
    pub async fn run(&self) -> Result<()> {
        loop {
            tokio::time::sleep(self.cycle_interval).await;
            self.run_cycle().await?;
        }
    }
    
    async fn run_cycle(&self) -> Result<ConsolidationReport> {
        // 1. Episodic Memories abrufen, die decay > 0.3 haben
        let episodics = self.collection
            .query()
            .memory_type(MemoryType::Episodic)
            .min_decay(0.3)
            .top_k(50)
            .execute().await?;
        
        // 2. Cluster episodics nach Community Detection (Label Propagation)
        let clusters = self.cluster_memories(&episodics).await?;
        
        // 3. Jeder Cluster → LLM Zusammenfassung → Semantic Memory
        for cluster in clusters {
            let summary = self.llm.summarize(&cluster).await?;
            let semantic = ContextChunk {
                memory_type: MemoryType::Semantic,
                importance: cluster.avg_importance(),
                ..ContextChunk::from_summary(summary)
            };
            // 4. Semantic Memory einfügen
            self.collection.insert_with_context(semantic).await?;
            // 5. Episodic Memories supprimieren (nicht löschen, mit Supersedes-Link)
            for episodic in &cluster.members {
                self.collection.relate(
                    semantic.id.clone(), episodic.id.clone(),
                    MemoryLink { relation: LinkRelation::Supersedes, strength: 1.0 }
                ).await?;
            }
        }
        Ok(ConsolidationReport { clusters_processed: clusters.len() })
    }
}
```

**Konfiguration** (in `MemFuse::open()` konfigurierbar):
```rust
MemFuseConfig {
    sleep_cycle: SleepCycleConfig {
        enabled: true,
        interval: Duration::from_secs(4 * 3600),
        min_episodic_threshold: 20,
        max_cluster_size: 10,
    }
}
```

#### Arbeitspaket 3-1-B: Verified Forgetting — Kryptographischer Löschbeweis (ADR-056)
**Dauer**: 4–5 Tage

**Konzept**: GDPR-Compliance — nachweisbares Löschen via Merkle-Invalidierung

```rust
/// Kryptographischer Löschbeweis
pub struct DeletionProof {
    pub doc_id: DocId,
    pub deletion_timestamp: TxId,
    pub merkle_path: Vec<Hash>,  // Pfad zum Blatt in Merkle-Tree
    pub nullifier: Hash,         // H(key || nonce) — beweist Löschung ohne Inhalt preiszugeben
    pub signature: Signature,    // HMAC-Signatur des Proof
}

impl Collection {
    /// Löscht ein Dokument und gibt kryptographischen Beweis zurück
    pub async fn delete_with_proof(&self, doc_id: &DocId) -> Result<DeletionProof> {
        // 1. Dokument aus allen Indizes löschen (standard delete_op)
        self.delete_op(doc_id).await?;
        // 2. Merkle-Tree Update: Blatt invalidieren
        let merkle_path = self.merkle_tree.invalidate(doc_id).await?;
        // 3. Nullifier berechnen: H(doc_key || deletion_nonce)
        let nullifier = blake3::hash(&[doc_id.as_bytes(), &rand::random::<[u8; 32]>()].concat());
        // 4. Proof signieren (HMAC mit WAL-Integrity-Key)
        Ok(DeletionProof { doc_id: doc_id.clone(), merkle_path, nullifier, .. })
    }
    
    /// Verifiziert einen Löschbeweis ohne Dokument-Inhalt zu enthüllen
    pub fn verify_deletion(&self, proof: &DeletionProof) -> bool { /* ... */ }
}
```

---

### Sprint 3-2: Advanced Graph Retrieval (Woche 5–8, 05.12.2026–02.01.2027)

#### Arbeitspaket 3-2-A: PathRAG — Relationale Pfadextraktion (ADR-057)
**Dauer**: 5–7 Tage  
**Betroffene Crate**: `memfuse-graph`

**Konzept**: Sucht nicht nur nächste Nachbarn, sondern extrahiert semantisch kohärente Pfade durch den Wissensgraphen.

```rust
pub enum GraphTraversalStrategy {
    PersonalizedPageRank { alpha: f32, max_hops: usize },  // ✅ Existiert (ADR-026)
    CommunityDetection { algorithm: CommunityAlgorithm },  // ✅ Existiert (ADR-027)
    PathExtraction {  // 🆕 NEU (PathRAG)
        source_entity: EntityId,
        target_entities: Vec<EntityId>,
        max_path_length: usize,
        scoring: PathScoringFunction,
    },
    CausalChain {  // 🆕 NEU (CausalEdge)
        effect: EntityId,
        max_depth: usize,
    }
}

/// PathRAG: Findet alle semantisch kohärenten Pfade
pub async fn extract_paths(
    &self,
    source: &EntityId,
    targets: &[EntityId],
    max_length: usize,
) -> Result<Vec<GraphPath>> {
    // Dijkstra / BFS mit Edge-Gewicht-Scoring
    // Pfade nach kombinierten Edge-Stärken sortiert
    // Max: max_length Hops
}

pub struct GraphPath {
    pub nodes: Vec<EntityId>,
    pub edges: Vec<EdgeId>,
    pub total_strength: f32,
    pub path_narrative: Option<String>,  // LLM-generierte Erklärung des Pfads
}
```

#### Arbeitspaket 3-2-B: CausalEdge — 4. Graph-Dimension (ADR-058)
**Dauer**: 3–4 Tage

```rust
/// Ergänzt bestehende Relationen um kausale Dimension
pub enum LinkRelation {
    // Existierende Relationen (ADR-038):
    Supports,
    Contradicts,
    Elaborates,
    Supersedes,
    References,
    // NEU: Kausale Dimension
    Causes,         // A verursacht B
    Enables,        // A ermöglicht B (schwächere Kausalität)
    Prevents,       // A verhindert B
    Correlates,     // A korreliert mit B (keine Kausalität, nur Assoziation)
}

// In CSR-Graph: CausalEdge mit Stärke + Konfidenz
pub struct CausalEdge {
    pub source: EntityId,
    pub target: EntityId,
    pub relation: LinkRelation,
    pub strength: f32,      // 0.0–1.0
    pub confidence: f32,    // 0.0–1.0 (LLM-Konfidenz bei Extraktion)
    pub evidence: Vec<DocId>, // Quell-Dokumente die Kausalität belegen
}
```

#### Arbeitspaket 3-2-C: Rekursive Relation Following (ADR-059)
**Dauer**: 2–3 Tage

```rust
/// Adaptive PPR mit automatischer Terminierung
pub async fn recursive_relation_follow(
    &self,
    start: &EntityId,
    decay_threshold: f32,  // Terminiere wenn Score < threshold
) -> Result<Vec<(EntityId, f32)>> {
    let mut visited = HashSet::new();
    let mut queue = BinaryHeap::new();
    queue.push((1.0_f32, start.clone()));
    
    while let Some((score, entity)) = queue.pop() {
        if score < decay_threshold { break; }
        if !visited.insert(entity.clone()) { continue; }
        
        let neighbors = self.get_neighbors(&entity).await?;
        for (neighbor, edge_weight) in neighbors {
            let propagated_score = score * edge_weight * self.config.damping_factor;
            if propagated_score >= decay_threshold {
                queue.push((propagated_score, neighbor));
            }
        }
    }
    Ok(visited.into_iter().map(|e| (e, 0.0)).collect()) // TODO: score propagation
}
```

---

### Sprint 3-3: Multi-Step Query mit Chain-of-Thought (Woche 9–12, 02.01.–06.02.2027)

#### Arbeitspaket 3-3-A: Query Chain-of-Thought Reasoning (ADR-060)
**Dauer**: 5–7 Tage  
**Betroffene Crate**: `memfuse-db` (MultiStepEngine), `memfuse-ollama`

**Ist-Zustand:**
```
MultiStepEngine rewritet Query bis zu 3× — ohne Erklärung, ohne Reasoning-Spur
```

**Soll-Zustand:**
```rust
pub struct ChainOfThoughtEngine {
    llm: Arc<OllamaClient>,
    max_rounds: usize,
    reasoning_model: String,  // z.B. "llama3.2:8b-reasoning"
}

impl ChainOfThoughtEngine {
    pub async fn reason_and_search(
        &self,
        collection: &Collection,
        user_query: &str,
    ) -> Result<(Vec<SearchResult>, ReasoningTrace)> {
        let mut trace = ReasoningTrace::new(user_query);
        let mut current_query = user_query.to_string();
        let mut all_results = Vec::new();
        
        for round in 0..self.max_rounds {
            // 1. LLM reasoning: "Was suche ich eigentlich? Was fehlt noch?"
            let reasoning = self.llm.complete(&format!(
                "Round {round}. Previous results: {prev}. Original question: {q}. \
                 What specific information am I still missing? What should I search for next?",
                prev = all_results.len(), q = user_query
            )).await?;
            
            // 2. LLM extrahiert neue Suchterme aus Reasoning
            let refined_query = self.llm.extract_search_query(&reasoning).await?;
            trace.add_round(round, &reasoning, &refined_query);
            
            // 3. Suche mit verfeinerten Terms
            let results = collection.search(&refined_query, 10).await?;
            all_results.extend(results);
            
            // 4. Terminierungsbedingung: LLM sagt "Ich habe genug Information"
            if self.llm.has_sufficient_information(&all_results, user_query).await? {
                break;
            }
        }
        
        Ok((deduplicate_and_rerank(all_results), trace))
    }
}

pub struct ReasoningTrace {
    pub original_query: String,
    pub rounds: Vec<ReasoningRound>,
    pub total_documents_retrieved: usize,
}
```

---

## 5. Phase 4 — Enterprise & Compliance (07.02. – 30.06.2027, 14 Wochen)

**Ziel**: MemFuse für Enterprise-Kunden bereit machen — RBAC, Compliance, Multi-Tenant, Observability.

---

### Sprint 4-1: OAuth 2.0 + RBAC (Woche 1–6, 07.02.–21.03.2027)

#### Arbeitspaket 4-1-A: JWT-basierte Authentifizierung in MCP-Server (ADR-061)
**Dauer**: 6–8 Tage  
**Betroffene Crate**: `memfuse-mcp`, `memfuse-core`

```rust
// memfuse-core/src/auth.rs (neues Modul)
pub struct JwtClaims {
    pub sub: String,      // Principal-ID (user/service account)
    pub roles: Vec<Role>, // ["admin", "reader", "writer"]
    pub tenant_id: String, // Für Multi-Tenant
    pub exp: u64,         // Expiry
}

pub enum Role {
    Admin,   // Alle Operationen
    Writer,  // Insert, Update, Delete
    Reader,  // Search, Get, List
}

// memfuse-mcp/src/server.rs
// Vor jedem Tool-Dispatch: JWT-Validierung
async fn call_tool(&self, name: &str, args: Value) -> Result<Value> {
    let claims = self.auth.validate_jwt(self.current_jwt.as_deref())?;
    let permission = tool_permission_required(name);
    self.auth.check_permission(&claims, permission)?;
    // ... dispatch
}
```

**MCP-Server OAuth-Flow:**
```
Client                 MCP Server              Authorization Server
  |                        |                          |
  |-- tool_call (no JWT)-->|                          |
  |<-- AuthRequired -------|                          |
  |-- OAuth2 PKCE flow --->|---------------auth_code->|
  |                        |<------ access_token -----|
  |-- retry with JWT ----->|                          |
  |<-- tool_result --------|                          |
```

#### Arbeitspaket 4-1-B: RBAC & Per-Tenant Collection Isolation
**Dauer**: 5–7 Tage

```rust
// Tenant-Namespace: {tenant_id}:{collection_name}
// Collections werden automatisch per Tenant isoliert

pub struct TenantConfig {
    pub tenant_id: String,
    pub max_collections: usize,
    pub max_vectors_per_collection: usize,
    pub allowed_models: Vec<String>,
}

// memfuse-db/src/lib.rs
pub async fn collection_for_tenant(
    &self,
    tenant_id: &str,
    collection_name: &str,
    claims: &JwtClaims,
) -> Result<Collection> {
    // Verifiziert: claims.tenant_id == tenant_id
    // Namespace: "{tenant_id}:{collection_name}"
    let namespaced = format!("{tenant_id}:{collection_name}");
    self.collection(&namespaced).await
}
```

---

### Sprint 4-2: Immutable Audit-Trail (Woche 5–8, 07.03.–04.04.2027)

#### Arbeitspaket 4-2-A: Merkle-basierter Audit-Trail (ADR-062)
**Dauer**: 6–8 Tage

```rust
// Jede Mutation generiert einen Audit-Log-Eintrag
pub struct AuditEntry {
    pub timestamp: SystemTime,
    pub tx_id: TxId,
    pub principal: String,     // JWT Subject
    pub tenant_id: String,
    pub operation: AuditOperation,
    pub doc_id: Option<DocId>,
    pub collection: String,
    pub previous_hash: Hash,    // Hash des vorherigen Eintrags (Kette)
    pub entry_hash: Hash,       // H(timestamp || operation || doc_id || previous_hash)
}

pub enum AuditOperation {
    Insert, Update, Delete, Search, CollectionCreate, CollectionDrop,
}

// Persistenz: Append-only Log in LSM-Store
// Key: __audit:{tenant_id}:{timestamp_nanos}
// Wert: bincode::serialize(AuditEntry)
// Unveränderlichkeit: WAL-HMAC-Chaining schützt vor Nachträglicher Manipulation

// API:
pub async fn audit_log_range(
    &self,
    tenant_id: &str,
    from: SystemTime,
    to: SystemTime,
) -> Result<Vec<AuditEntry>> { /* scan_prefix mit Zeitbereich */ }

pub fn verify_audit_chain(&self, entries: &[AuditEntry]) -> bool {
    // Verifiziert: jedes entry.previous_hash == hash(entries[i-1])
    entries.windows(2).all(|w| w[1].previous_hash == w[0].entry_hash)
}
```

---

### Sprint 4-3: Observability & Production Infrastructure (Woche 9–12, 05.04.–03.05.2027)

#### Arbeitspaket 4-3-A: OpenTelemetry Tracing (ADR-063)
**Dauer**: 4–5 Tage

```toml
# Cargo.toml (optional feature "telemetry")
[features]
telemetry = ["opentelemetry", "opentelemetry-otlp", "tracing-opentelemetry"]
```

```rust
// Instrumentierung der kritischen Pfade:
#[tracing::instrument(
    name = "hybrid_search",
    skip(self, query_embedding),
    fields(collection = %collection_name, top_k = top_k)
)]
pub async fn hybrid_search(&self, ...) -> Result<Vec<SearchResult>> {
    let span = tracer.start("hybrid_search");
    
    // Sub-spans für jeden Signal-Kanal:
    let vector_span = span.start_child("vector_search");
    let vector_results = self.hnsw.search(query_embedding, top_k).await?;
    vector_span.finish();
    
    // RRF Fusion:
    let fusion_span = span.start_child("rrf_fusion");
    let fused = reciprocal_rank_fusion(&[vector_results, text_results, graph_results]);
    fusion_span.finish();
    
    span.finish();
    Ok(fused)
}
```

**Exportierbare Spans:**
- `hybrid_search` (mit Sub-spans pro Signal)
- `diskann_build` (Rebuild-Dauer)
- `sleep_cycle_consolidation` (Konsolidierungs-Runde)
- `mcp_tool_call` (welches Tool, Latenz)
- `ollama_embed` + `ollama_completion` (LLM-Latenz)

#### Arbeitspaket 4-3-B: Prometheus Metrics Exposition (ADR-064)
**Dauer**: 3–4 Tage

```rust
// Metriken (über feature "metrics"):
lazy_static! {
    static ref HYBRID_SEARCH_LATENCY: Histogram = register_histogram!(
        "memfuse_hybrid_search_latency_seconds",
        "Latency of hybrid search operation",
        vec![0.000_010, 0.000_050, 0.000_100, 0.001, 0.010, 0.100]
    ).unwrap();
    
    static ref ACTIVE_COLLECTIONS: Gauge = register_gauge!(
        "memfuse_active_collections_total",
        "Number of active collections"
    ).unwrap();
    
    static ref PROVENANCE_COMPLETENESS: Gauge = register_gauge!(
        "memfuse_provenance_completeness_ratio",
        "Ratio of search results with complete provenance"
    ).unwrap();
    
    static ref DLQ_SIZE: Counter = register_counter!(
        "memfuse_dead_letter_queue_total",
        "Total items in dead letter queue"
    ).unwrap();
    
    static ref SLEEP_CYCLE_CONSOLIDATIONS: Counter = register_counter!(
        "memfuse_sleep_cycle_consolidations_total",
        "Total sleep cycle consolidation runs"
    ).unwrap();
}
```

---

### Sprint 4-4: Key Rotation & Encryption Hardening (Woche 11–12, 26.04.–10.05.2027)

#### Arbeitspaket 4-4-A: Rolling Key Rotation (ADR-065)
**Dauer**: 4–5 Tage

```rust
// Vorher: Einmaliger Key bei DB-Init
// Nachher: Versionierter Key mit Rolling-Scheme

pub struct KeyVersionStore {
    current_version: u32,
    keys: HashMap<u32, MasterKey>,  // Version → Key
}

impl KeyVersionStore {
    /// Rotiert zu neuem Schlüssel; re-encrypts alle SSTables im Hintergrund
    pub async fn rotate_key(&mut self) -> Result<u32> {
        let new_key = MasterKey::generate();
        let new_version = self.current_version + 1;
        self.keys.insert(new_version, new_key);
        self.current_version = new_version;
        // Hintergrund-Task: SSTables mit altem Key → neuen Key re-encrypten
        spawn_rekey_task(self.clone()).await;
        Ok(new_version)
    }
    
    /// Liest Key für gegebene SSTable-Version
    pub fn key_for_version(&self, version: u32) -> Option<&MasterKey> {
        self.keys.get(&version)
    }
}
```

---

## 6. Eigenentwicklungen für Gold Standard (Priorisiert)

**Frage**: Was muss zwingend eigenentwickelt werden (kein existierendes Open-Source-Tool geeignet)?

### Kategorie A — MUSS Eigenentwicklung (keine Alternative)

| Komponente | Begründung | Umfang |
|---|---|---|
| **HNSW Copy-on-Write Rebuild** | Spezifisch für MemFuse MVCC-TxId-Integration | 4–6 Tage |
| **4-Signal ProvenanceRecord** | MemFuse-spezifische 4-Signal-Fusion | 2–3 Tage |
| **Sleep-Cycle Konsolidierung** | Spezifisch für Episodic→Semantic-Workflow | 6–8 Tage |
| **Verified Forgetting / DeletionProof** | Eigen-Crypto mit WAL-Integration | 4–5 Tage |
| **Tenant-Namespace-Isolation in LSM** | Direkte LSM-Store-Integration | 3–4 Tage |
| **Audit-Chain mit WAL-HMAC** | Nutzt bestehenden HMAC-Mechanismus | 6–8 Tage |

### Kategorie B — BESSER als Eigenentwicklung (generische Tools zu schwer anpassbar)

| Komponente | Alternative | Warum eigene Implementierung besser |
|---|---|---|
| **DiskANN Lifecycle** | Open-DiskANN (C++) | Pure-Rust + MVCC-Integration + MemFuse-Mmap-Format |
| **PathRAG** | networkx (Python) | Rust-native + Async + CSR-Format nativ |
| **CausalEdge** | Neo4j APOC | Air-Gapped, keine JVM-Abhängigkeit |
| **BM25 + Deutsche Morphologie** | Elasticsearch | Embedded ohne Netzwerk, eigene Compound Splitter |
| **Conformal Calibration Router** | scikit-learn | Rust-native, kein Python FFI benötigt |

### Kategorie C — KANN externe Bibliotheken nutzen (aber Pure-Rust-Alternativen bevorzugen)

| Komponente | Empfohlene Crate | Bedingung |
|---|---|---|
| JWT Validierung | `jsonwebtoken` (Rust) | Pure-Rust ✅ |
| OAuth 2.0 | `oauth2` (Rust) | Pure-Rust ✅ |
| Prometheus Metrics | `prometheus` (Rust) | Pure-Rust ✅ |
| OpenTelemetry | `opentelemetry` (Rust) | Pure-Rust ✅ |
| GraphQL Schema | `async-graphql` (Rust) | Pure-Rust ✅ (Phase 4+) |

**Policy**: Alle neuen Crates müssen `#![forbid(unsafe_code)]`-kompatibel sein und dürfen keine C-Abhängigkeiten im Default-Feature-Set haben (ADR-004).

---

## 7. Metriken & Tracking

### Performance-Ziele (Phase 2B Exit-Kriterien)

| Metrik | Baseline (03.09.2026) | Ziel (07.11.2026) | Messmethode |
|---|---|---|---|
| Hybrid Search Latency (p50) | 12,78 µs | < 15 µs | `cargo bench -p memfuse-db` |
| Hybrid Search Latency (p95) | ~25 µs | < 20 µs | Criterion-Benchmark |
| Max Latency Spike (HNSW Rebuild) | 5.000–30.000 ms | < 50 ms | Lasttest mit parallelen Inserts |
| Batch Insert Throughput (10k docs) | ~1k docs/s | > 20k docs/s | Batch-Bench |
| Router Kalibrierungskonvergenz | Oszillierend | Stabil ±2% nach 50 Entscheidungen | Simulator-Test |
| ProvenanceRecord-Vollständigkeit | 0% | 100% | Unit-Test-Coverage |
| Agent Step Erfolgsrate | ~95% | > 99% (mit Dead-Letter+Retry) | Stresstest |
| DiskANN Query @ 1M Vektoren | N/A | < 200 ms (p95) | DiskANN-Bench |
| Competitive: 4-Signal vs. Vector-only | Baseline | ≥ 30% besserer Recall@10 | Benchmark-Suite |

### Qualitäts-Gates (unverändert, ADR-Pflicht)

```bash
# Nach jedem Commit/PR:
cargo check --workspace --exclude memfuse-tauri
cargo test --workspace --exclude memfuse-tauri    # 0 Failures
just check                                         # Clippy + rustfmt
just triple-test                                   # Flaky-Test-Detektor
just dag-check                                     # Layer DAG Integrität
cargo xtask validate-tags                          # ISO-8601 Tags korrekt
```

---

## 8. Risiken & Mitigationsstrategien

| Risiko | Wahrscheinlichkeit | Auswirkung | Mitigation |
|---|---|---|---|
| HNSW Delta-Merge hat semantischen Bug | Mittel | Hoch | Proptest: Alle Inserts vor/nach Watermark müssen suchbar sein |
| DiskANN Integration dauert >8 Wochen | Mittel | Mittel | Early-Exit: HNSW-Limit bei 1M Vektoren dokumentieren, DiskANN als Phase 3 verschieben |
| Router-Fix verschlechtert Routing-Qualität | Niedrig | Mittel | A/B-Test: 50 Requests im Simulator vor/nach Fix vergleichen |
| Sleep-Cycle LLM-Summarization zu teuer | Mittel | Mittel | Rate-Limiting: max 100 Summarizations pro Zyklus; optionales Throttling |
| OAuth-Implementierung verletzt ADR-004 | Niedrig | Hoch | `jsonwebtoken` crate ist Pure-Rust; vor Hinzufügen prüfen |
| Benchmark-Suite belastet Entwicklungszeit | Niedrig | Niedrig | Benchmarks parallel zu Phase-2B-Fixes entwickeln |
| Ollama als Backend gibt es nicht mehr | Sehr Niedrig | Hoch | Abstraktions-Trait `Embedder` ermöglicht Swap zu llama.cpp oder LMStudio |

---

## 9. ADR-Nummernplan

> ⚠️ **Pflicht**: Vor Vergabe einer ADR-Nummer immer live prüfen:  
> `grep -oP '(?<=^## ADR-)\d+' DECISIONS.md | sort -n | tail -1`

| ADR | Titel | Sprint | Status |
|---|---|---|---|
| ADR-048 | HNSW Copy-on-Write Rebuild | 2B-1-A | 📋 Zu erstellen |
| ADR-049 | Unified Conformal Calibration | 2B-1-B | 📋 Zu erstellen |
| ADR-050 | Context Compaction OCC Retry & Crash-Recovery | 2B-1-C | 📋 Zu erstellen |
| ADR-051 | ProvenanceRecord End-to-End Befüllung | 2B-2-A | 📋 Zu erstellen |
| ADR-052 | Agent Dead-Letter, Timeout, Budget | 2B-2-B | 📋 Zu erstellen |
| ADR-053 | Batch-Pfade Layer 2/3 | 2B-2-C | 📋 Zu erstellen |
| ADR-054 | DiskANN Production Lifecycle | 2B-3 | 📋 Zu erstellen |
| ADR-055 | Sleep-Cycle Memory Consolidation | 3-1-A | 📋 Zu erstellen |
| ADR-056 | Verified Forgetting & DeletionProof | 3-1-B | 📋 Zu erstellen |
| ADR-057 | PathRAG Graphpfad-Extraktion | 3-2-A | 📋 Zu erstellen |
| ADR-058 | CausalEdge Graphdimension | 3-2-B | 📋 Zu erstellen |
| ADR-059 | Rekursive Relation Following | 3-2-C | 📋 Zu erstellen |
| ADR-060 | Chain-of-Thought Multi-Step Query | 3-3-A | 📋 Zu erstellen |
| ADR-061 | JWT-RBAC in MCP-Server | 4-1-A | 📋 Zu erstellen |
| ADR-062 | Immutable Audit-Trail | 4-2-A | 📋 Zu erstellen |
| ADR-063 | OpenTelemetry Tracing | 4-3-A | 📋 Zu erstellen |
| ADR-064 | Prometheus Metrics | 4-3-B | 📋 Zu erstellen |
| ADR-065 | Rolling Key Rotation | 4-4-A | 📋 Zu erstellen |

---

## 10. Ergebnis-Milesteine (Gates)

### Gate 1 — Sprint 2B-1 abgeschlossen (26.09.2026)
- [ ] HNSW Rebuild Spike < 50 ms (Bench-verifiziert)
- [ ] Router Kalibrierung konvergiert (50-Sample-Test)
- [ ] Context Compaction Retry implementiert und getestet
- [ ] ADR-048, 049, 050 in DECISIONS.md

### Gate 2 — Sprint 2B-2 abgeschlossen (17.10.2026)
- [ ] ProvenanceRecord in 100% der Search-Results befüllt
- [ ] Agent Dead-Letter Queue im LSM-Store persistiert
- [ ] `memfuse_batch_insert` MCP-Tool verfügbar
- [ ] `insert_many()` 10× schneller als N Einzel-Inserts (gemessen)
- [ ] ADR-051, 052, 053 in DECISIONS.md

### Gate 3 — Sprint 2B-3 abgeschlossen (07.11.2026)
- [ ] DiskANN Index baut aus HNSW auf (1M Vektoren, < 30 min)
- [ ] HybridVectorIndex Search-Konsistenz verifiziert
- [ ] Graceful Degradation bei korrupter .diskann-Datei
- [ ] ADR-054 in DECISIONS.md
- [ ] **Release 0.2.0**: PyPI + crates.io veröffentlicht

### Gate 4 — Sprint 2B-4 & Phase 2B vollständig (07.11.2026)
- [ ] Alle 9 Lücken (L-1 bis L-9) geschlossen
- [ ] Competitive Benchmark Suite dokumentiert (MemFuse vs. Mem0 vs. Chroma)
- [ ] PyPI `memfuse 0.2.0` mit vollständigen PyO3-Bindings
- [ ] Blog-Post: "Why MemFuse is different" veröffentlicht

### Gate 5 — Phase 3 abgeschlossen (06.02.2027)
- [ ] Sleep-Cycle Konsolidierung läuft als Hintergrunddaemon
- [ ] Verified Forgetting: DeletionProof verifizierbar
- [ ] PathRAG: Graph-Pfade in SearchResult sichtbar
- [ ] CausalEdge in Relation-Schema
- [ ] Chain-of-Thought Multi-Step demonstrierbar in Demo-App

### Gate 6 — Phase 4 abgeschlossen = Gold Standard (30.06.2027)
- [ ] OAuth 2.0 + RBAC in MCP-Server
- [ ] Multi-Tenant Collection Isolation
- [ ] Immutable Audit-Trail mit Merkle-Verifikation
- [ ] OpenTelemetry Traces in Jaeger exportierbar
- [ ] Prometheus Metrics Dashboard (Grafana-Template im Repo)
- [ ] Rolling Key Rotation mit automatischem Re-keying
- [ ] **Release 1.0.0**: Stabile API-Garantien, vollständige Dokumentation
- [ ] **Enterprise Preview**: Self-hosted Installer für DACH-Unternehmen

---

## 11. Zusammenfassung & Prioritäten

### TOP-3 Sofortmaßnahmen (Nächste 2 Wochen)

**1. ADR-049 (Router Fix) — 1 Tag**  
Einfachste Verbesserung mit sofortigem algorithmischem Nutzen. Router konvergiert stabil nach < 50 Entscheidungen statt zu oszillieren.

**2. ADR-048 (HNSW Rebuild) — 4–6 Tage**  
Kritischster Bottleneck für Produktion. Eliminiert 5–30s Latenz-Spikes unter Last. Ohne diesen Fix kann kein real-world Workload sustained werden.

**3. ADR-051 (ProvenanceRecord) — 2–3 Tage**  
Unblockiert Enterprise-Sales sofort: "Warum wurde dieses Dokument angezeigt?" ist eine häufige erste Compliance-Frage. Verhältnismäßig geringer Aufwand für großen strategischen Nutzen.

### Was MemFuse Gold Standard macht

MemFuse ist kein universelles Allround-System. Es dominiert in einem spezifischen Segment:

> **Lokal. Air-Gapped. Hybrid. Auditable.**

Kein anderes System kombiniert: Pure-Rust + 4-Signal-Fusion + ACID-Garantien + Contextual Retrieval + Session DAG + Kryptographische Löschbeweise + Cognitive Memory + MCP-Native + Zero Infrastructure.

Das ist der Gold Standard — nicht maximale Feature-Liste, sondern maximale Kohärenz aller Features in einem einzigen, einbettbaren System.

---

*Dokument-Status: LIVING DOCUMENT — wird nach jedem abgeschlossenen Sprint aktualisiert*  
*Nächste Review: Nach Gate 1 (26.09.2026)*  
*Verantwortlich: Lead Architect + AI Development Team*
