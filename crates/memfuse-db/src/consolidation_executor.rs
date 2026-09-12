// FILE-CONTEXT
// ZWECK: Verbindet Structural Consolidation Pass-Ergebnisse mit der Collection-Mutation-API.
// INVARIANTEN: Nur Structural Consolidation Pass (rein strukturell) hier. Keine LLM-Calls. Keine direkte Abhängigkeit zu memfuse-graph (P1-DAG-Integrität).
// STAND: TS:2026-09-07T08:30:00Z

//! Verbindet Structural Consolidation Pass-Ergebnisse mit der Collection-Mutation-API.
//! INVARIANTE: Nur Structural Consolidation Pass (rein strukturell) hier. Keine LLM-Calls.

use crate::collection::{Collection, StoredDocumentMeta};
use crate::memory_consolidation::{
    compute_community_hash, run_consolidation_pass, run_structural_synthesis_pass,
    CommunityStabilityTracker, ConsolidationConfig, ConsolidationPhaseResult, SynthesisConfig,
    SynthesisPhaseResult,
};
use memfuse_core::traits::{
    LlmTextGenerator, ResponseGroundingValidator, StorageEngine, VectorIndex,
};
use memfuse_core::{DocId, Result};
use memfuse_graph::{detect_communities, CommunityDetectionConfig};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// RAII Guard zur Koordination des Konsolidierungslaufs.
/// Stellt sicher, dass das `consolidation_in_progress`-Flag bei Beendigung oder Panic
/// stets panic-sicher auf `false` zurückgesetzt wird.
pub struct ConsolidationLockGuard {
    flag: Arc<AtomicBool>,
}

impl ConsolidationLockGuard {
    /// Versucht, das Konsolidierungs-Lock atomic zu erwerben (`compare_exchange`).
    /// Gibt `Some(ConsolidationLockGuard)` zurück, wenn das Lock erfolgreich reserviert wurde.
    /// Gibt `None` zurück, falls bereits ein Konsolidierungslauf aktiv ist.
    pub fn try_acquire(flag: &Arc<AtomicBool>) -> Option<Self> {
        if flag
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
            .is_ok()
        {
            Some(Self { flag: flag.clone() })
        } else {
            None
        }
    }
}

impl Drop for ConsolidationLockGuard {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::Release);
    }
}

/// Führt den Structural Consolidation Pass aus UND wendet die Ergebnisse an (Tombstones, Graph-Cascade).
///
/// Gibt das `ConsolidationPhaseResult` zurück.
pub async fn execute_consolidation_pass<S: StorageEngine, V: VectorIndex>(
    collection: &Collection<S, V>,
    turns: &[(DocId, Vec<f32>)],
    config: &ConsolidationConfig,
) -> Result<ConsolidationPhaseResult> {
    if turns.is_empty() {
        return Ok(ConsolidationPhaseResult {
            segments_created: 0,
            duplicates_tombstoned: Vec::new(),
            cascade_edge_tombstones_needed: Vec::new(),
            cascade_errors: Vec::new(),
        });
    }

    // Erwerbe consolidation_guard per try_lock(), um parallele Durchläufe auf derselben Collection zu verhindern (ADR-081)
    let _guard = match collection.consolidation_guard().try_lock() {
        Ok(guard) => guard,
        Err(_) => {
            tracing::warn!(
                collection = %collection.name(),
                "Konsolidierungsdurchlauf übersprungen — anderer Pfad aktiv (H-19-Schutz)"
            );
            return Ok(ConsolidationPhaseResult {
                segments_created: 0,
                duplicates_tombstoned: Vec::new(),
                cascade_edge_tombstones_needed: Vec::new(),
                cascade_errors: Vec::new(),
            });
        }
    };

    let mut result = run_consolidation_pass(turns, config);

    // Tombstones auf echte Collection anwenden
    for doc_id in &result.duplicates_tombstoned {
        let doc_key = collection.namespaced_key(&doc_id.inner().to_le_bytes(), 1);
        let user_id = match collection.storage().get(&doc_key).await {
            Ok(Some(val)) => match serde_json::from_slice::<StoredDocumentMeta>(&val) {
                Ok(meta) => meta.id,
                Err(e) => {
                    tracing::warn!(doc_id = ?doc_id, error = %e, "Consolidation pass: failed to deserialize StoredDocumentMeta for tombstone");
                    continue;
                }
            },
            Ok(None) => {
                tracing::warn!(doc_id = ?doc_id, "Consolidation pass: doc_id key not found for tombstone");
                continue;
            }
            Err(e) => {
                tracing::warn!(doc_id = ?doc_id, error = %e, "Consolidation pass: failed to lookup doc_id for tombstone");
                continue;
            }
        };

        match collection.delete(&user_id).await {
            Ok(_) => {
                tracing::debug!(doc_id = ?doc_id, user_id = %user_id, "Consolidation pass: duplicate tombstoned")
            }
            Err(e) => {
                tracing::warn!(doc_id = ?doc_id, user_id = %user_id, error = %e, "Consolidation pass: tombstone failed")
            }
        }
    }

    // Kaskadierende Graph-Edge-Tombstones für supersedete Dokumente anwenden (INV-GRAPH-PROV-1)
    if !result.cascade_edge_tombstones_needed.is_empty() {
        let tx = collection.allocate_tx()?;
        for doc_id in &result.cascade_edge_tombstones_needed {
            match memfuse_graph::cascade_invalidate_edges_for_superseded_doc(
                &collection.graph_index,
                *doc_id,
                tx.inner(),
            )
            .await
            {
                Ok(report) => {
                    tracing::debug!(
                        doc_id = ?doc_id,
                        tombstoned_edges = report.tombstoned_edge_count,
                        "Consolidation pass: cascade edge invalidation successful"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        doc_id = ?doc_id,
                        error = %e,
                        "Consolidation pass: cascade edge invalidation failed"
                    );
                    result
                        .cascade_errors
                        .push(format!("DocId {:?}: {}", doc_id, e));
                }
            }
        }
    }

    Ok(result)
}

/// Führt die vollständige Hintergrund-Konsolidierung (Structural Consolidation Pass und optional Generative Synthesis Pass) aus.
///
/// 1. Structural Consolidation Pass: Segmentierung & Near-Duplicate Tombstoning.
/// 2. Generative Synthesis Pass (falls `synthesis_config` und `llm` angegeben): Wissenssynthese über stabile Graph-Communities.
///    Synthetisierte MetaChunks werden in die Collection eingefügt.
pub async fn execute_background_consolidation<S: StorageEngine, V: VectorIndex>(
    collection: &Collection<S, V>,
    turns: &[(DocId, Vec<f32>)],
    consolidation_config: &ConsolidationConfig,
    synthesis_config: Option<&SynthesisConfig>,
    llm: Option<&dyn LlmTextGenerator>,
    validator: Option<&dyn ResponseGroundingValidator>,
    stability_tracker: Option<&mut CommunityStabilityTracker>,
) -> Result<(ConsolidationPhaseResult, Option<SynthesisPhaseResult>)> {
    let consolidation_result =
        execute_consolidation_pass(collection, turns, consolidation_config).await?;

    let synthesis_result = if let (Some(synth_cfg), Some(llm_gen)) = (synthesis_config, llm) {
        let assignments = detect_communities(
            &collection.graph_index,
            &CommunityDetectionConfig::default(),
        )
        .await?;

        // Group entities by community_id
        let mut comm_map: HashMap<u64, Vec<DocId>> = HashMap::new();
        for assignment in assignments {
            let doc_id = DocId::new(assignment.entity_id.inner());
            comm_map
                .entry(assignment.community_id)
                .or_default()
                .push(doc_id);
        }

        let mut currently_observed = HashSet::new();
        let mut stable_communities = Vec::new();

        let mut local_tracker;
        let tracker = match stability_tracker {
            Some(t) => t,
            None => {
                local_tracker = CommunityStabilityTracker::new();
                &mut local_tracker
            }
        };

        for (_comm_id, members) in comm_map {
            let comm_hash = compute_community_hash(&members);
            currently_observed.insert(comm_hash);
            let count = tracker.observe(comm_hash);
            if count >= synth_cfg.stability_cycles_required {
                stable_communities.push((comm_hash, members));
            }
        }

        tracker.reset_if_absent(&currently_observed);

        // Fetch source texts for all members in stable_communities
        let mut source_texts: HashMap<DocId, String> = HashMap::new();
        for (_comm_hash, members) in &stable_communities {
            for doc_id in members {
                if source_texts.contains_key(doc_id) {
                    continue;
                }
                let doc_key = collection.namespaced_key(&doc_id.inner().to_le_bytes(), 1);
                if let Ok(Some(val)) = collection.storage().get(&doc_key).await {
                    if let Ok(meta) = serde_json::from_slice::<StoredDocumentMeta>(&val) {
                        if let Ok(Some(doc)) = collection.get(&meta.id).await {
                            if let Some(meta_val) = doc.metadata {
                                if let Some(text_val) =
                                    meta_val.get("text").and_then(|v| v.as_str())
                                {
                                    source_texts.insert(*doc_id, text_val.to_string());
                                    continue;
                                }
                            }
                            source_texts.insert(*doc_id, meta.id.clone());
                            continue;
                        }
                    }
                }
                source_texts.insert(*doc_id, doc_id.inner().to_string());
            }
        }

        // Structural Consolidation Pass über stabile Communities ausführen.
        // HINWEIS: Grounding-Validierung wird ausgeführt, wenn ein `ResponseGroundingValidator` (z. B. `GaspValidator` aus dem Candle-Backend) übergeben wird.
        // Bei LLM-Backends ohne GaspValidator-Unterstützung ist validator `None` und die Validierung wird übersprungen.
        let synth_res = run_structural_synthesis_pass(
            &stable_communities,
            &source_texts,
            llm_gen,
            synth_cfg,
            validator,
        )
        .await?;

        for (idx, meta_chunk) in synth_res.synthesized.iter().enumerate() {
            let chunk_id = format!("rem_synth_{}_{}", meta_chunk.source_community_hash, idx);
            let source_ids_json: Vec<u64> = meta_chunk
                .abstracts_from
                .iter()
                .map(|id| id.inner())
                .collect();
            let metadata = serde_json::json!({
                "rem_synthesized": true,
                "source_community_hash": meta_chunk.source_community_hash,
                "source_doc_ids": source_ids_json,
                "model_id": meta_chunk.llm_model_id,
                "text": meta_chunk.content,
            });

            if let Err(e) = collection
                .insert_text_only(&chunk_id, &meta_chunk.content, Some(metadata.clone()))
                .await
            {
                tracing::debug!(chunk_id = %chunk_id, error = %e, "insert_text_only failed; storing synthesized chunk via put_kv");
                let _ = collection.put_kv(&chunk_id, &metadata).await;
            }
        }

        Some(synth_res)
    } else {
        None
    };

    Ok((consolidation_result, synthesis_result))
}

/// Deprecated legacy wrapper for `execute_background_consolidation`.
#[deprecated(note = "use execute_background_consolidation instead")]
pub async fn execute_sleep_cycle<S: StorageEngine, V: VectorIndex>(
    collection: &Collection<S, V>,
    turns: &[(DocId, Vec<f32>)],
    consolidation_config: &ConsolidationConfig,
    synthesis_config: Option<&SynthesisConfig>,
    llm: Option<&dyn LlmTextGenerator>,
    stability_tracker: Option<&mut CommunityStabilityTracker>,
) -> Result<(ConsolidationPhaseResult, Option<SynthesisPhaseResult>)> {
    execute_background_consolidation(
        collection,
        turns,
        consolidation_config,
        synthesis_config,
        llm,
        None,
        stability_tracker,
    )
    .await
}

/// Tokio-Background-Task Engine für periodische Speicher-Konsolidierung und Wissenssynthese.
pub struct ConsolidationEngine<S: StorageEngine, V: VectorIndex = memfuse_index::HnswIndex> {
    collection: Arc<Collection<S, V>>,
    llm: Option<Arc<dyn LlmTextGenerator>>,
    validator: Option<Arc<dyn ResponseGroundingValidator>>,
    consolidation_config: ConsolidationConfig,
    synthesis_config: SynthesisConfig,
    interval: std::time::Duration,
    cancel_token: tokio_util::sync::CancellationToken,
    stability_tracker: Arc<tokio::sync::Mutex<CommunityStabilityTracker>>,
}

impl<S: StorageEngine + 'static, V: VectorIndex + 'static> ConsolidationEngine<S, V> {
    /// Erstellt eine neue `ConsolidationEngine`-Instanz.
    pub fn new(
        collection: Arc<Collection<S, V>>,
        consolidation_config: ConsolidationConfig,
        synthesis_config: SynthesisConfig,
        interval: std::time::Duration,
        cancel_token: tokio_util::sync::CancellationToken,
    ) -> Self {
        Self {
            collection,
            llm: None,
            validator: None,
            consolidation_config,
            synthesis_config,
            interval,
            cancel_token,
            stability_tracker: Arc::new(tokio::sync::Mutex::new(CommunityStabilityTracker::new())),
        }
    }

    /// Fügt ein optionales LLM für die generative Wissenssynthese hinzu.
    pub fn with_llm(mut self, llm: Arc<dyn LlmTextGenerator>) -> Self {
        self.llm = Some(llm);
        self
    }

    /// Fügt einen optionalen Grounding-Validator für den Generative Synthesis Pass hinzu.
    pub fn with_validator(mut self, validator: Arc<dyn ResponseGroundingValidator>) -> Self {
        self.validator = Some(validator);
        self
    }

    /// Startet die Engine als Tokio-Background-Task.
    pub fn start(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            self.run().await;
        })
    }

    /// Hauptschleife der `ConsolidationEngine`.
    pub async fn run(&self) {
        let mut ticker = tokio::time::interval(self.interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        tracing::info!(
            collection = %self.collection.name(),
            interval = ?self.interval,
            "ConsolidationEngine background task started"
        );

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    if let Err(err) = self.run_cycle().await {
                        tracing::error!(
                            collection = %self.collection.name(),
                            error = %err,
                            "ConsolidationEngine: cycle execution failed"
                        );
                    }
                }
                _ = self.cancel_token.cancelled() => {
                    tracing::info!(
                        collection = %self.collection.name(),
                        "ConsolidationEngine shutting down via cancellation token"
                    );
                    break;
                }
            }
        }
    }

    /// Führt einen einzelnen Konsolidierungszyklus aus.
    pub async fn run_cycle(
        &self,
    ) -> Result<(ConsolidationPhaseResult, Option<SynthesisPhaseResult>)> {
        // P14-Compliance: Koordination zwischen ConsolidationEngine und MaintenanceScheduler.
        // Versuche das Konsolidierungs-Lock atomic zu erwerben. Bei Konflikt überspringe diesen Lauf.
        let _guard = match ConsolidationLockGuard::try_acquire(
            &self.collection.consolidation_in_progress(),
        ) {
            Some(g) => g,
            None => {
                tracing::debug!(
                    collection = %self.collection.name(),
                    "consolidation_in_progress, skipping trigger"
                );
                return Ok((
                    ConsolidationPhaseResult {
                        segments_created: 0,
                        duplicates_tombstoned: Vec::new(),
                        cascade_edge_tombstones_needed: Vec::new(),
                        cascade_errors: Vec::new(),
                    },
                    None,
                ));
            }
        };

        // 1. Turns chronologisch aus der Collection lesen
        let user_key_prefix = self.collection.user_key_prefix();
        let entries = match self
            .collection
            .storage()
            .scan_prefix(&user_key_prefix)
            .await
        {
            Ok(e) => e,
            Err(err) => {
                tracing::error!(
                    collection = %self.collection.name(),
                    error = %err,
                    "ConsolidationEngine: scan_prefix failed"
                );
                return Err(err);
            }
        };

        let mut turns: Vec<(DocId, Vec<f32>)> = Vec::new();
        for (k, v) in entries {
            if self.collection.name() == "default" && k.starts_with(b"__") {
                continue;
            }
            if let Ok(stored) = serde_json::from_slice::<crate::collection::StoredDocument>(&v) {
                if let Ok(doc_id) = DocId::from_key(&stored.id) {
                    turns.push((doc_id, stored.embedding));
                }
            }
        }

        if turns.is_empty() {
            return Ok((
                ConsolidationPhaseResult {
                    segments_created: 0,
                    duplicates_tombstoned: Vec::new(),
                    cascade_edge_tombstones_needed: Vec::new(),
                    cascade_errors: Vec::new(),
                },
                None,
            ));
        }

        let mut tracker_guard = self.stability_tracker.lock().await;
        let llm_ref = self
            .llm
            .as_ref()
            .map(|l| l.as_ref() as &dyn LlmTextGenerator);
        let validator_ref = self
            .validator
            .as_ref()
            .map(|v| v.as_ref() as &dyn ResponseGroundingValidator);

        let (consolidation_res, synthesis_res) = execute_background_consolidation(
            self.collection.as_ref(),
            &turns,
            &self.consolidation_config,
            Some(&self.synthesis_config),
            llm_ref,
            validator_ref,
            Some(&mut *tracker_guard),
        )
        .await?;

        // APM-Datenverlust-Bei-Absturz:
        // Synthetisierte Summaries wurden bereits in execute_background_consolidation erzeugt & persistiert.
        // Erst NACH dem Bestätigen der Summaries wird der Decay Controller angewendet.
        if synthesis_res.is_some() {
            let decay_controller =
                crate::decay_controller::AdaptiveDecayController::with_defaults();
            let _ = self
                .collection
                .evict_decayed_chunks(&decay_controller, 100)
                .await;
        }

        Ok((consolidation_res, synthesis_res))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use memfuse_core::traits::LlmTextGenerator;
    use memfuse_core::BoxFuture;
    use memfuse_graph::CsrGraph;
    use memfuse_index::HnswIndex;
    use memfuse_store::LsmStorage;
    use serde_json::json;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::Arc;
    use std::time::Duration;
    use tempfile::tempdir;

    struct TestMockLlm {
        should_fail: AtomicBool,
        call_count: AtomicU64,
    }

    impl TestMockLlm {
        fn new() -> Self {
            Self {
                should_fail: AtomicBool::new(false),
                call_count: AtomicU64::new(0),
            }
        }
    }

    impl LlmTextGenerator for TestMockLlm {
        fn generate<'a>(&'a self, _prompt: &'a str) -> BoxFuture<'a, memfuse_core::Result<String>> {
            Box::pin(async move {
                self.call_count.fetch_add(1, Ordering::SeqCst);
                if self.should_fail.load(Ordering::SeqCst) {
                    Err(memfuse_core::MemFuseError::Internal(
                        "Simulated LLM Fault Injection Failure".to_string(),
                    ))
                } else {
                    Ok("Synthesized summary text for community.".to_string())
                }
            })
        }
    }

    async fn create_test_collection() -> (Arc<Collection<LsmStorage, HnswIndex>>, tempfile::TempDir)
    {
        let dir = tempdir().expect("tempdir");
        let storage = Arc::new(
            LsmStorage::new(memfuse_store::LsmConfig {
                path: dir.path().to_path_buf(),
                ..Default::default()
            })
            .await
            .expect("LsmStorage"),
        );
        let index = Arc::new(
            HnswIndex::try_new(memfuse_index::HnswConfig {
                dimension: 4,
                ..Default::default()
            })
            .expect("HnswIndex"),
        );
        let graph = Arc::new(CsrGraph::new());
        let next_tx = Arc::new(AtomicU64::new(1));

        let col = Arc::new(Collection::new(
            "default".to_string(),
            storage,
            index,
            graph,
            next_tx,
            4,
            memfuse_text::Language::English,
        ));
        (col, dir)
    }

    #[tokio::test]
    async fn test_consolidation_engine_concurrency_no_deadlock() {
        let (col, _dir) = create_test_collection().await;
        let cancel_token = tokio_util::sync::CancellationToken::new();

        let engine = Arc::new(ConsolidationEngine::new(
            col.clone(),
            ConsolidationConfig {
                near_duplicate_cosine_threshold: 0.9999,
                ..Default::default()
            },
            SynthesisConfig::default(),
            Duration::from_millis(20),
            cancel_token.clone(),
        ));

        // Perform parallel foreground insert/search operations with distinct embeddings
        let mut insert_handles = Vec::new();
        for i in 0..20 {
            let col_clone = col.clone();
            insert_handles.push(tokio::spawn(async move {
                let id = format!("fg_doc_{}", i);
                let angle = (i as f32) * std::f32::consts::PI / 10.0;
                let vec = vec![
                    angle.cos(),
                    angle.sin(),
                    (angle * 2.0).cos(),
                    (angle * 2.0).sin(),
                ];
                let _ = col_clone.insert(&id, &vec, Some(json!({"fg": true}))).await;
                let _ = col_clone.query().embedding(&vec).k(5).execute().await;
            }));
        }

        for h in insert_handles {
            h.await.expect("join handle");
        }

        let engine_handle = engine.clone().start();

        // Run explicit cycle to confirm completion
        let res = engine.run_cycle().await;
        assert!(res.is_ok(), "run_cycle should succeed under concurrency");

        cancel_token.cancel();
        let _ = engine_handle.await;

        // Confirm foreground docs exist
        assert_eq!(col.len().await, 20);
    }

    #[tokio::test]
    async fn test_consolidation_engine_fault_injection_resume() {
        let (col, _dir) = create_test_collection().await;
        let cancel_token = tokio_util::sync::CancellationToken::new();

        let llm = Arc::new(TestMockLlm::new());

        // Setup graph entities & communities for synthesis pass with non-duplicate embeddings
        col.insert_typed(
            "turn_1",
            &[1.0, 0.0, 0.0, 0.0],
            memfuse_core::MemoryType::Episodic,
            Some(json!({"text": "Turn 1 content"})),
        )
        .await
        .expect("insert 1");

        col.insert_typed(
            "turn_2",
            &[0.0, 1.0, 0.0, 0.0],
            memfuse_core::MemoryType::Episodic,
            Some(json!({"text": "Turn 2 content"})),
        )
        .await
        .expect("insert 2");

        col.relate("turn_1", "turn_2", "co_occurrence")
            .await
            .expect("relate");

        let engine = Arc::new(
            ConsolidationEngine::new(
                col.clone(),
                ConsolidationConfig::default(),
                SynthesisConfig {
                    min_community_size: 2,
                    stability_cycles_required: 1,
                    max_llm_calls_per_cycle: 10,
                    min_grounding_score: None,
                },
                Duration::from_secs(60),
                cancel_token.clone(),
            )
            .with_llm(llm.clone()),
        );

        // Cycle 1: Inject fault
        llm.should_fail.store(true, Ordering::SeqCst);
        let cycle1 = engine.run_cycle().await;
        assert!(
            cycle1.is_ok(),
            "Cycle 1 should handle LLM failure gracefully"
        );
        let (_cons1, synth1) = cycle1.unwrap();
        if let Some(s1) = synth1 {
            assert_eq!(
                s1.synthesized.len(),
                0,
                "No chunks synthesized due to LLM error"
            );
        }

        // Cycle 2: Clear fault and resume
        llm.should_fail.store(false, Ordering::SeqCst);
        let cycle2 = engine.run_cycle().await;
        assert!(cycle2.is_ok(), "Cycle 2 should succeed after fault cleared");
        let (_cons2, synth2) = cycle2.unwrap();
        if let Some(s2) = synth2 {
            assert_eq!(
                s2.synthesized.len(),
                1,
                "Synthesized 1 meta chunk after fault resolution"
            );
        }

        // Cycle 3: Re-run cycle to prove idempotency (no duplicate summaries)
        let cycle3 = engine.run_cycle().await;
        assert!(cycle3.is_ok());

        cancel_token.cancel();
    }

    #[tokio::test]
    async fn test_execute_consolidation_pass_skips_when_guard_locked() {
        let (col, _dir) = create_test_collection().await;

        let turn1 = (DocId::new(1), vec![1.0, 0.0, 0.0, 0.0]);
        let turn2 = (DocId::new(2), vec![1.0, 0.0, 0.0, 0.0]);
        let turns = vec![turn1, turn2];

        let config = ConsolidationConfig {
            near_duplicate_cosine_threshold: 0.99,
            ..Default::default()
        };

        // Lock guard manually
        let _guard = col.consolidation_guard().try_lock().expect("try_lock");

        let res = execute_consolidation_pass(col.as_ref(), &turns, &config).await;
        assert!(res.is_ok(), "Should return Ok when guard is locked");
        let phase_res = res.unwrap();
        assert_eq!(phase_res.segments_created, 0);
        assert!(phase_res.duplicates_tombstoned.is_empty());
    }
}
