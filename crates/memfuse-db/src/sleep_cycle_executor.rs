// FILE-CONTEXT
// ZWECK: Verbindet NREM-Phase-Ergebnisse mit der Collection-Mutation-API.
// INVARIANTEN: Nur NREM (rein strukturell) hier. Keine LLM-Calls. Keine direkte Abhängigkeit zu memfuse-graph (P1-DAG-Integrität).
// STAND: TS:2026-09-07T08:30:00Z

//! Verbindet NREM-Phase-Ergebnisse mit der Collection-Mutation-API.
//! INVARIANTE: Nur NREM (rein strukturell) hier. Keine LLM-Calls.

use crate::collection::{Collection, StoredDocumentMeta};
use crate::sleep_cycle::{
    compute_community_hash, run_nrem_phase, run_rem_phase, CommunityStabilityTracker, NremConfig,
    NremPhaseResult, RemConfig, RemPhaseResult,
};
use memfuse_core::traits::{LlmTextGenerator, StorageEngine, VectorIndex};
use memfuse_core::{DocId, Result};
use memfuse_graph::{detect_communities, CommunityDetectionConfig};
use std::collections::{HashMap, HashSet};

/// Führt NREM-Phase aus UND wendet die Ergebnisse an (Tombstones, Graph-Cascade).
///
/// Gibt das `NremPhaseResult` zurück.
pub async fn execute_nrem_cycle<S: StorageEngine, V: VectorIndex>(
    collection: &Collection<S, V>,
    turns: &[(DocId, Vec<f32>)],
    config: &NremConfig,
) -> Result<NremPhaseResult> {
    if turns.is_empty() {
        return Ok(NremPhaseResult {
            segments_created: 0,
            duplicates_tombstoned: Vec::new(),
            cascade_edge_tombstones_needed: Vec::new(),
        });
    }

    let result = run_nrem_phase(turns, config);

    // Tombstones auf echte Collection anwenden
    for doc_id in &result.duplicates_tombstoned {
        let doc_key = collection.namespaced_key(&doc_id.inner().to_le_bytes(), 1);
        let user_id = match collection.storage().get(&doc_key).await {
            Ok(Some(val)) => match serde_json::from_slice::<StoredDocumentMeta>(&val) {
                Ok(meta) => meta.id,
                Err(e) => {
                    tracing::warn!(doc_id = ?doc_id, error = %e, "NREM: failed to deserialize StoredDocumentMeta for tombstone");
                    continue;
                }
            },
            Ok(None) => {
                tracing::warn!(doc_id = ?doc_id, "NREM: doc_id key not found for tombstone");
                continue;
            }
            Err(e) => {
                tracing::warn!(doc_id = ?doc_id, error = %e, "NREM: failed to lookup doc_id for tombstone");
                continue;
            }
        };

        match collection.delete(&user_id).await {
            Ok(_) => {
                tracing::debug!(doc_id = ?doc_id, user_id = %user_id, "NREM: duplicate tombstoned")
            }
            Err(e) => {
                tracing::warn!(doc_id = ?doc_id, user_id = %user_id, error = %e, "NREM: tombstone failed")
            }
        }
    }

    // Graph-Cascade: nur loggen (Implementierung in memfuse-graph Crate-Grenze)
    // INVARIANTE P1-DAG: Crate ruft memfuse-graph nicht direkt an (Zyklen vermeiden)
    if !result.cascade_edge_tombstones_needed.is_empty() {
        tracing::info!(
            count = result.cascade_edge_tombstones_needed.len(),
            "NREM: cascade graph edge tombstones needed — caller must invoke graph cleanup"
        );
    }

    Ok(result)
}

/// Führt den vollständigen Sleep-Cycle (NREM-Phase und optional REM-Phase) aus.
///
/// 1. NREM-Phase: Segmentierung & Near-Duplicate Tombstoning.
/// 2. REM-Phase (falls `rem_config` und `llm` angegeben): Wissenssynthese über stabile Graph-Communities.
///    Synthetisierte MetaChunks werden in die Collection eingefügt.
pub async fn execute_sleep_cycle<S: StorageEngine>(
    collection: &Collection<S>,
    turns: &[(DocId, Vec<f32>)],
    nrem_config: &NremConfig,
    rem_config: Option<&RemConfig>,
    llm: Option<&dyn LlmTextGenerator>,
    stability_tracker: Option<&mut CommunityStabilityTracker>,
) -> Result<(NremPhaseResult, Option<RemPhaseResult>)> {
    let nrem_result = execute_nrem_cycle(collection, turns, nrem_config).await?;

    let rem_result = if let (Some(rem_cfg), Some(llm_gen)) = (rem_config, llm) {
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
            if count >= rem_cfg.stability_cycles_required {
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

        let rem_res = run_rem_phase(&stable_communities, &source_texts, llm_gen, rem_cfg).await?;

        for (idx, meta_chunk) in rem_res.synthesized.iter().enumerate() {
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

        Some(rem_res)
    } else {
        None
    };

    Ok((nrem_result, rem_result))
}
