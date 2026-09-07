// FILE-CONTEXT
// ZWECK: Verbindet NREM-Phase-Ergebnisse mit der Collection-Mutation-API.
// INVARIANTEN: Nur NREM (rein strukturell) hier. Keine LLM-Calls. Keine direkte Abhängigkeit zu memfuse-graph (P1-DAG-Integrität).
// STAND: TS:2026-09-07T08:30:00Z

//! Verbindet NREM-Phase-Ergebnisse mit der Collection-Mutation-API.
//! INVARIANTE: Nur NREM (rein strukturell) hier. Keine LLM-Calls.

use crate::collection::{Collection, StoredDocumentMeta};
use crate::rem_phase::{run_rem_phase, RemPhaseResult, SegmentSynthesizer};
use crate::sleep_cycle::{group_turns_into_segments, run_nrem_phase, NremConfig, NremPhaseResult};
use memfuse_core::traits::StorageEngine;
use memfuse_core::{DocId, Result};

/// Führt NREM-Phase aus UND wendet die Ergebnisse an (Tombstones, Graph-Cascade).
///
/// Gibt das `NremPhaseResult` zurück.
pub async fn execute_nrem_cycle<S: StorageEngine>(
    collection: &Collection<S>,
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
/// 2. REM-Phase (falls `rem_synthesizer` angegeben): Generative Wissenssynthese pro Segment.
///    Erzeugte synthetische Chunks werden in die Collection eingefügt.
pub async fn execute_sleep_cycle<S: StorageEngine>(
    collection: &Collection<S>,
    turns: &[(DocId, Vec<f32>)],
    config: &NremConfig,
    rem_synthesizer: Option<&dyn SegmentSynthesizer>,
) -> Result<(NremPhaseResult, Option<RemPhaseResult>)> {
    let nrem_result = execute_nrem_cycle(collection, turns, config).await?;

    let rem_result = if let Some(synthesizer) = rem_synthesizer {
        if turns.is_empty() {
            Some(RemPhaseResult {
                synthesized_chunks: Vec::new(),
                skipped_segments: 0,
            })
        } else {
            let segments = group_turns_into_segments(turns, config);
            let mut segment_texts = Vec::with_capacity(segments.len());

            for seg in &segments {
                let mut texts_in_seg = Vec::with_capacity(seg.turn_ids.len());
                for doc_id in &seg.turn_ids {
                    let doc_key = collection.namespaced_key(&doc_id.inner().to_le_bytes(), 1);
                    if let Ok(Some(val)) = collection.storage().get(&doc_key).await {
                        if let Ok(meta) = serde_json::from_slice::<StoredDocumentMeta>(&val) {
                            if let Ok(Some(doc)) = collection.get(&meta.id).await {
                                if let Some(meta_val) = doc.metadata {
                                    if let Some(text_val) =
                                        meta_val.get("text").and_then(|v| v.as_str())
                                    {
                                        texts_in_seg.push(text_val.to_string());
                                        continue;
                                    }
                                }
                                texts_in_seg.push(meta.id.clone());
                                continue;
                            }
                        }
                    }
                    texts_in_seg.push(doc_id.inner().to_string());
                }
                segment_texts.push(texts_in_seg);
            }

            let rem_res = run_rem_phase(
                &segments,
                &segment_texts,
                synthesizer,
                config.min_turns_per_segment,
            )
            .await;

            for (idx, chunk) in rem_res.synthesized_chunks.iter().enumerate() {
                let chunk_id = format!("rem_synth_{}_{}", synthesizer.model_id(), idx);
                let source_turn_ids_json: Vec<u64> =
                    chunk.source_turn_ids.iter().map(|id| id.inner()).collect();
                let metadata = serde_json::json!({
                    "rem_synthesized": true,
                    "source_turn_count": chunk.source_turn_ids.len(),
                    "source_turn_ids": source_turn_ids_json,
                    "model_id": chunk.model_id,
                    "text": chunk.content,
                });

                if let Err(e) = collection
                    .insert_text_only(&chunk_id, &chunk.content, Some(metadata.clone()))
                    .await
                {
                    tracing::debug!(chunk_id = %chunk_id, error = %e, "insert_text_only failed; storing synthesized chunk via put_kv");
                    let _ = collection.put_kv(&chunk_id, &metadata).await;
                }
            }

            Some(rem_res)
        }
    } else {
        None
    };

    Ok((nrem_result, rem_result))
}
