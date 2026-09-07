// FILE-CONTEXT
// ZWECK: Verbindet NREM-Phase-Ergebnisse mit der Collection-Mutation-API.
// INVARIANTEN: Nur NREM (rein strukturell) hier. Keine LLM-Calls. Keine direkte Abhängigkeit zu memfuse-graph (P1-DAG-Integrität).
// STAND: TS:2026-09-07T08:30:00Z

//! Verbindet NREM-Phase-Ergebnisse mit der Collection-Mutation-API.
//! INVARIANTE: Nur NREM (rein strukturell) hier. Keine LLM-Calls.

use crate::collection::{Collection, StoredDocumentMeta};
use crate::sleep_cycle::{run_nrem_phase, NremConfig, NremPhaseResult};
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
