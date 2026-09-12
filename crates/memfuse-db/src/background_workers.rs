// FILE-CONTEXT
// ZWECK: Hintergrund-Worker-Tasks zur TTL-Löschung, Entropie-Pruning und Bereinigung verwaister Transaktionen (Orphan Cleanup).
// INVARIANTEN: Geordnete Abschaltung via CancellationToken; Beschränkung der pro Tick verarbeiteten Elemente.
// NICHT-OFFENSICHTLICH: Orphan Cleanup Worker triggert bei HNSW-Indextrennung automatischen Rebuild mit Timeout.
// STAND: TS:2026-08-29T17:22:29Z (SESSION: 0dcb9f3b)

use crate::collection::{Collection, StoredDocument};
use crate::consolidation_executor::{execute_consolidation_pass, ConsolidationLockGuard};
use crate::memory_consolidation::ConsolidationConfig;
use memfuse_core::traits::StorageEngine;
use memfuse_core::tx_buffer::TxBuffer;
use memfuse_core::DocId;
#[cfg(feature = "background-maintenance")]
use memfuse_core::VectorIndex;
use std::sync::Arc;
use std::time::Duration;

#[cfg(feature = "background-maintenance")]
use crate::decay_controller::{AdaptiveDecayController, DecayControllerConfig};

/// Maximum number of orphan transactions processed in a single worker tick
/// to avoid starving foreground operations.
pub const MAX_ORPHANS_PER_TICK: usize = 100;

/// Maximum number of expired documents processed in a single expiry cleanup tick.
pub const MAX_EXPIRED_PER_TICK: usize = 100;

/// Starts a background task for periodic consolidation.
#[deprecated(
    note = "Konsolidiert in MaintenanceScheduler — siehe maintenance_scheduler.rs. Wird nach Migrationsfrist entfernt."
)]
pub fn start_consolidation_worker<S: StorageEngine>(
    collection: Arc<Collection<S>>,
    consolidation_config: ConsolidationConfig,
    interval: Duration,
    cancel_token: tokio_util::sync::CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        tracing::info!(
            collection = %collection.name(),
            interval = ?interval,
            "Consolidation worker task started"
        );

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    // P14-Compliance: Koordination mit ConsolidationEngine und MaintenanceScheduler
                    let _guard = match ConsolidationLockGuard::try_acquire(
                        &collection.consolidation_in_progress(),
                    ) {
                        Some(g) => g,
                        None => {
                            tracing::debug!(
                                collection = %collection.name(),
                                "consolidation_in_progress, skipping trigger"
                            );
                            continue;
                        }
                    };

                    // Extract turns from collection (chronologically sorted)
                    let user_key_prefix = collection.user_key_prefix();
                    let entries = match collection.storage().scan_prefix(&user_key_prefix).await {
                        Ok(entries) => entries,
                        Err(err) => {
                            tracing::error!(
                                collection = %collection.name(),
                                error = %err,
                                "Consolidation worker: failed to scan collection"
                            );
                            continue;
                        }
                    };

                    let mut turns: Vec<(DocId, Vec<f32>)> = Vec::new();
                    for (k, v) in entries {
                        if collection.name() == "default" && k.starts_with(b"__") {
                            continue;
                        }
                        if let Ok(stored) = serde_json::from_slice::<StoredDocument>(&v) {
                            if let Ok(doc_id) = DocId::from_key(&stored.id) {
                                turns.push((doc_id, stored.embedding));
                            }
                        }
                    }

                    if turns.is_empty() {
                        continue;
                    }

                    match execute_consolidation_pass(&collection, &turns, &consolidation_config).await {
                        Ok(res) => {
                            if !res.duplicates_tombstoned.is_empty() {
                                tracing::info!(
                                    collection = %collection.name(),
                                    tombstoned = res.duplicates_tombstoned.len(),
                                    segments = res.segments_created,
                                    "Consolidation worker: consolidated duplicate turns"
                                );
                            }
                        }
                        Err(err) => {
                            tracing::error!(
                                collection = %collection.name(),
                                error = %err,
                                "Consolidation worker: pass execution failed"
                            );
                        }
                    }
                }
                _ = cancel_token.cancelled() => {
                    tracing::info!(
                        collection = %collection.name(),
                        "Consolidation worker task shutting down via token"
                    );
                    break;
                }
            }
        }
    })
}

/// Deprecated legacy alias for `start_consolidation_worker`.
#[deprecated(note = "use start_consolidation_worker instead")]
#[allow(deprecated)]
pub fn start_consolidation_reaper<S: StorageEngine>(
    collection: Arc<Collection<S>>,
    consolidation_config: ConsolidationConfig,
    interval: Duration,
    cancel_token: tokio_util::sync::CancellationToken,
) -> tokio::task::JoinHandle<()> {
    start_consolidation_worker(collection, consolidation_config, interval, cancel_token)
}

/// Starts a background task to periodically clean up expired documents with TTL.
pub fn start_expiry_cleanup_worker<S: StorageEngine>(
    collection: Arc<Collection<S>>,
    interval: Duration,
    cancel_token: tokio_util::sync::CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        tracing::info!(
            collection = %collection.name(),
            interval = ?interval,
            "Expiry cleanup worker task started"
        );
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    match collection.reap_expired_documents(MAX_EXPIRED_PER_TICK).await {
                        Ok(reaped) if reaped > 0 => {
                            tracing::info!(
                                collection = %collection.name(),
                                reaped = reaped,
                                "Expiry cleanup worker cleaned up expired documents"
                            );
                        }
                        Ok(_) => {}
                        Err(err) => {
                            tracing::error!(
                                collection = %collection.name(),
                                error = %err,
                                "Error during expiry cleanup worker execution"
                            );
                        }
                    }
                }
                _ = cancel_token.cancelled() => {
                    tracing::info!(
                        collection = %collection.name(),
                        "Expiry cleanup worker task shutting down via token"
                    );
                    break;
                }
            }
        }
    })
}

/// Deprecated legacy alias for `start_expiry_cleanup_worker`.
#[deprecated(note = "use start_expiry_cleanup_worker instead")]
pub fn start_expiry_reaper<S: StorageEngine>(
    collection: Arc<Collection<S>>,
    interval: Duration,
    cancel_token: tokio_util::sync::CancellationToken,
) -> tokio::task::JoinHandle<()> {
    start_expiry_cleanup_worker(collection, interval, cancel_token)
}

/// Starts a background task for decay-controller-driven importance-score eviction.
/// Nur aktiv wenn `background-maintenance` Feature-Flag gesetzt.
#[cfg(feature = "background-maintenance")]
pub fn start_decay_cleanup_worker<S: StorageEngine, V: VectorIndex>(
    collection: Arc<Collection<S, V>>,
    decay_config: DecayControllerConfig,
    interval: Duration,
    cancel_token: tokio_util::sync::CancellationToken,
) -> tokio::task::JoinHandle<()> {
    let decay_controller = AdaptiveDecayController::new(decay_config);
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    match collection.evict_decayed_chunks(&decay_controller, 100).await {
                        Ok(n) if n > 0 => tracing::info!(evicted = n, "Decay controller worker evicted chunks"),
                        Ok(_) => {},
                        Err(e) => tracing::error!(error = %e, "Decay controller worker error"),
                    }
                }
                _ = cancel_token.cancelled() => break,
            }
        }
    })
}

/// Deprecated legacy alias for `start_decay_cleanup_worker`.
#[cfg(feature = "background-maintenance")]
#[deprecated(note = "use start_decay_cleanup_worker instead")]
pub fn start_thermostat_reaper<S: StorageEngine, V: VectorIndex>(
    collection: Arc<Collection<S, V>>,
    decay_config: DecayControllerConfig,
    interval: Duration,
    cancel_token: tokio_util::sync::CancellationToken,
) -> tokio::task::JoinHandle<()> {
    start_decay_cleanup_worker(collection, decay_config, interval, cancel_token)
}

/// Starts a background task to periodically clean up orphan transactions.
///
/// This worker handles the cleanup of transactions that have exceeded their
/// configured timeout without being committed or rolled back.
pub fn start_orphan_cleanup_worker<T: Clone + Send + Sync + 'static>(
    buffer: Arc<TxBuffer<T>>,
    hnsw_index: Arc<memfuse_index::hnsw::HnswIndex>,
    interval: Duration,
    cancel_token: tokio_util::sync::CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        tracing::info!(
            "Orphan cleanup worker started (timeout: {:?}, interval: {:?})",
            buffer.tx_timeout(),
            interval
        );
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    let buf = buffer.clone();
                    let expired = tokio::task::spawn_blocking(move || {
                        buf.reap_orphans_bounded(MAX_ORPHANS_PER_TICK)
                    })
                    .await
                    .unwrap_or_default();

                    if !expired.is_empty() {
                        tracing::warn!(
                            "Orphan cleanup worker cleaned up {} expired transactions",
                            expired.len()
                        );
                    }
                    // AI-TAG[SMELL][MAJOR] Unbounded HNSW rebuild loop without backoff in orphan worker (ID: AGT-DB-c42e91a0) (TS: 2026-09-12T18:43:13Z) (SESSION: e6ab3646)
                    if let Err(err) = hnsw_index.check_connectivity() {
                        tracing::warn!(
                            error = %err,
                            "HNSW index degraded — triggering automatic rebuild"
                        );
                        match tokio::time::timeout(Duration::from_secs(120), hnsw_index.rebuild()).await {
                            Ok(Ok(())) => {},
                            Ok(Err(rebuild_err)) => {
                                tracing::error!(error = %rebuild_err, "HNSW rebuild failed");
                            }
                            Err(_) => {
                                tracing::warn!("HNSW rebuild timed out after 120s; skipping this tick");
                            }
                        }
                    }
                }
                _ = cancel_token.cancelled() => {
                    tracing::info!("Orphan cleanup worker shutting down via token");
                    break;
                }
            }
        }
    })
}

/// Deprecated legacy alias for `start_orphan_cleanup_worker`.
#[deprecated(note = "use start_orphan_cleanup_worker instead")]
pub fn start_orphan_reaper<T: Clone + Send + Sync + 'static>(
    buffer: Arc<TxBuffer<T>>,
    hnsw_index: Arc<memfuse_index::hnsw::HnswIndex>,
    interval: Duration,
    cancel_token: tokio_util::sync::CancellationToken,
) -> tokio::task::JoinHandle<()> {
    start_orphan_cleanup_worker(buffer, hnsw_index, interval, cancel_token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use memfuse_core::tx_buffer::IndexOp;
    use memfuse_core::types::{DocId, TxId};
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_expiry_cleanup_worker_task_cleans_documents() {
        use memfuse_graph::CsrGraph;
        use memfuse_index::HnswIndex;
        use memfuse_store::LsmStorage;
        use std::sync::atomic::AtomicU64;
        use tempfile::tempdir;

        let dir = tempdir().unwrap(); // unwrap
        let storage = Arc::new(
            LsmStorage::new(memfuse_store::LsmConfig {
                path: dir.path().to_path_buf(),
                ..Default::default()
            })
            .await
            .unwrap(), // unwrap
        );
        let index = Arc::new(
            HnswIndex::try_new(memfuse_index::HnswConfig {
                dimension: 4,
                ..Default::default()
            })
            .unwrap(), // unwrap
        );
        let col = Arc::new(crate::Collection::new(
            "default".to_string(),
            storage,
            index,
            Arc::new(CsrGraph::new()),
            Arc::new(AtomicU64::new(1)),
            4,
            memfuse_text::Language::English,
        ));

        let vec = vec![1.0, 0.0, 0.0, 0.0];
        col.insert_with_ttl("doc_task_ttl", &vec, None, 2)
            .await
            .unwrap(); // unwrap

        // Perform 2 dummy commits
        col.insert("d1", &vec, None).await.unwrap(); // unwrap
        col.insert("d2", &vec, None).await.unwrap(); // unwrap

        let cancel_token = tokio_util::sync::CancellationToken::new();
        let handle = start_expiry_cleanup_worker(
            col.clone(),
            Duration::from_millis(10),
            cancel_token.clone(),
        );

        let mut cleaned = false;
        for _ in 0..50 {
            sleep(Duration::from_millis(10)).await;
            if col.get("doc_task_ttl").await.unwrap().is_none() {
                // unwrap
                cleaned = true;
                break;
            }
        }

        cancel_token.cancel();
        let _ = handle.await;

        assert!(
            cleaned,
            "Expiry cleanup worker task should delete expired document"
        );
    }

    #[tokio::test]
    async fn test_orphan_cleanup_worker_removes_expired() {
        let buffer = Arc::new(TxBuffer::<String>::new_with_config(
            64,
            Duration::from_millis(50),
        ));
        let tx1 = TxId::new(1);

        buffer.begin(tx1);
        let _ = buffer.stage(
            tx1,
            IndexOp::Insert {
                doc_id: DocId::new(1),
                data: "old".to_string(),
            },
        );

        let cancel_token = tokio_util::sync::CancellationToken::new();
        let config = memfuse_index::hnsw::HnswConfig::default();
        let hnsw_index = Arc::new(memfuse_index::hnsw::HnswIndex::try_new(config).unwrap()); // unwrap
        let _worker = start_orphan_cleanup_worker(
            buffer.clone(),
            hnsw_index.clone(),
            Duration::from_millis(10),
            cancel_token.clone(),
        );
        assert!(buffer.has_tx(tx1));

        let mut removed = false;
        for _ in 0..50 {
            sleep(Duration::from_millis(10)).await;
            if !buffer.has_tx(tx1) {
                removed = true;
                break;
            }
        }
        cancel_token.cancel();
        assert!(
            removed,
            "Expired transaction should have been cleaned up within 500ms"
        );
    }

    #[tokio::test]
    async fn trigger_expiry_cleanup_deletes_expired_documents() {
        use memfuse_graph::CsrGraph;
        use memfuse_index::HnswIndex;
        use memfuse_store::LsmStorage;
        use serde_json::json;
        use std::sync::atomic::AtomicU64;
        use tempfile::tempdir;

        let dir = tempdir().unwrap(); // unwrap
        let lsm_config = memfuse_store::LsmConfig {
            path: dir.path().to_path_buf(),
            ..Default::default()
        };
        let storage = Arc::new(LsmStorage::new(lsm_config).await.unwrap()); // unwrap
        let index = Arc::new(
            HnswIndex::try_new(memfuse_index::HnswConfig {
                dimension: 4,
                ..Default::default()
            })
            .unwrap(), // unwrap
        );
        let graph = Arc::new(CsrGraph::new());
        let next_tx = Arc::new(AtomicU64::new(1));

        let col = crate::Collection::new(
            "default".to_string(),
            storage,
            index,
            graph,
            next_tx,
            4,
            memfuse_text::Language::English,
        );

        let vec = vec![1.0, 0.0, 0.0, 0.0];
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap() // unwrap
            .as_millis() as u64;

        col.insert(
            "doc1",
            &vec,
            Some(json!({"created_at_ms": now_ms - 100, "ttl_ms": 50})),
        )
        .await
        .unwrap(); // unwrap

        col.trigger_expiry_cleanup().await.unwrap(); // unwrap
        let result = col.get("doc1").await.unwrap(); // unwrap
        assert!(result.is_none(), "Expired document must be deleted");
    }

    #[tokio::test]
    async fn test_worker_immediate_cancellation() {
        use memfuse_graph::CsrGraph;
        use memfuse_index::HnswIndex;
        use memfuse_store::LsmStorage;
        use std::sync::atomic::AtomicU64;
        use tempfile::tempdir;

        let dir = tempdir().unwrap(); // unwrap
        let storage = Arc::new(
            LsmStorage::new(memfuse_store::LsmConfig {
                path: dir.path().to_path_buf(),
                ..Default::default()
            })
            .await
            .unwrap(), // unwrap
        );
        let index = Arc::new(
            HnswIndex::try_new(memfuse_index::HnswConfig {
                dimension: 4,
                ..Default::default()
            })
            .unwrap(), // unwrap
        );
        let col = Arc::new(crate::Collection::new(
            "default".to_string(),
            storage,
            index,
            Arc::new(CsrGraph::new()),
            Arc::new(AtomicU64::new(1)),
            4,
            memfuse_text::Language::English,
        ));

        let cancel_token = tokio_util::sync::CancellationToken::new();
        cancel_token.cancel(); // cancel before starting

        let handle = start_expiry_cleanup_worker(col, Duration::from_secs(60), cancel_token);
        let res = handle.await;
        assert!(res.is_ok(), "Task should exit cleanly upon cancellation");
    }

    #[tokio::test]
    async fn test_decay_eviction_thresholds() {
        use crate::decay_controller::{AdaptiveDecayController, DecayControllerConfig};
        use memfuse_core::{DecayFunction, ImportanceScore, MemoryImportance, TxId};
        use memfuse_graph::CsrGraph;
        use memfuse_index::HnswIndex;
        use memfuse_store::LsmStorage;
        use serde_json::json;
        use std::sync::atomic::Ordering;
        use tempfile::tempdir;

        let dir = tempdir().unwrap(); // unwrap
        let storage = Arc::new(
            LsmStorage::new(memfuse_store::LsmConfig {
                path: dir.path().to_path_buf(),
                ..Default::default()
            })
            .await
            .unwrap(), // unwrap
        );
        let index = Arc::new(
            HnswIndex::try_new(memfuse_index::HnswConfig {
                dimension: 4,
                ..Default::default()
            })
            .unwrap(), // unwrap
        );
        let next_tx = Arc::new(std::sync::atomic::AtomicU64::new(1));

        let col = Arc::new(crate::Collection::new(
            "default".to_string(),
            storage,
            index.clone(),
            Arc::new(CsrGraph::new()),
            next_tx.clone(),
            4,
            memfuse_text::Language::English,
        ));

        let vec = vec![1.0, 0.0, 0.0, 0.0];

        // 1. Add 10 old low-score chunks
        for i in 0..10 {
            let id = format!("old_low_{i}");
            let imp = MemoryImportance::new(
                ImportanceScore::new(0.02),
                DecayFunction::Exponential { half_life_tx: 100 },
                TxId::new(1),
            );
            col.insert(&id, &vec, Some(json!({ "importance": imp })))
                .await
                .unwrap(); // unwrap
        }

        // 2. Add fresh high-score chunks
        for i in 0..5 {
            let id = format!("fresh_high_{i}");
            let imp = MemoryImportance::new(
                ImportanceScore::new(0.95),
                DecayFunction::Exponential {
                    half_life_tx: 100_000,
                },
                TxId::new(100_000),
            );
            col.insert(&id, &vec, Some(json!({ "importance": imp })))
                .await
                .unwrap(); // unwrap
        }

        // Advance current transaction ID to 50_000
        next_tx.store(50_000, Ordering::SeqCst);

        let decay_controller = AdaptiveDecayController::new(DecayControllerConfig {
            kappa: 2.0,
            base_half_life_tx: 1_000,
            eviction_threshold: 0.01,
        });

        // Reap with decay controller sweep
        let evicted = col
            .evict_decayed_chunks(&decay_controller, 100)
            .await
            .unwrap(); // unwrap
        assert_eq!(evicted, 10, "All 10 old low-score chunks should be evicted");

        // Verify old_low chunks are gone
        for i in 0..10 {
            let res = col.get(&format!("old_low_{i}")).await.unwrap(); // unwrap
            assert!(res.is_none(), "old_low_{i} must be deleted");
        }

        // Verify fresh_high chunks remain
        for i in 0..5 {
            let res = col.get(&format!("fresh_high_{i}")).await.unwrap(); // unwrap
            assert!(res.is_some(), "fresh_high_{i} must remain");
        }
    }

    #[cfg(feature = "background-maintenance")]
    #[tokio::test]
    async fn test_start_decay_cleanup_worker_background_task() {
        use crate::decay_controller::DecayControllerConfig;
        use memfuse_core::{DecayFunction, ImportanceScore, MemoryImportance, TxId};
        use memfuse_graph::CsrGraph;
        use memfuse_index::HnswIndex;
        use memfuse_store::LsmStorage;
        use serde_json::json;
        use std::sync::atomic::Ordering;
        use tempfile::tempdir;

        let dir = tempdir().unwrap(); // unwrap
        let storage = Arc::new(
            LsmStorage::new(memfuse_store::LsmConfig {
                path: dir.path().to_path_buf(),
                ..Default::default()
            })
            .await
            .unwrap(), // unwrap
        );
        let index = Arc::new(
            HnswIndex::try_new(memfuse_index::HnswConfig {
                dimension: 4,
                ..Default::default()
            })
            .unwrap(), // unwrap
        );
        let next_tx = Arc::new(std::sync::atomic::AtomicU64::new(1));

        let col = Arc::new(crate::Collection::new(
            "default".to_string(),
            storage,
            index,
            Arc::new(CsrGraph::new()),
            next_tx.clone(),
            4,
            memfuse_text::Language::English,
        ));

        let vec = vec![1.0, 0.0, 0.0, 0.0];
        let imp = MemoryImportance::new(
            ImportanceScore::new(0.01),
            DecayFunction::Exponential { half_life_tx: 10 },
            TxId::new(1),
        );
        col.insert("decay_target", &vec, Some(json!({ "importance": imp })))
            .await
            .unwrap(); // unwrap

        next_tx.store(100_000, Ordering::SeqCst);

        let cancel_token = tokio_util::sync::CancellationToken::new();
        let handle = start_decay_cleanup_worker(
            col.clone(),
            DecayControllerConfig::default(),
            Duration::from_millis(10),
            cancel_token.clone(),
        );

        let mut evicted = false;
        for _ in 0..50 {
            sleep(Duration::from_millis(10)).await;
            if col.get("decay_target").await.unwrap().is_none() {
                // unwrap
                evicted = true;
                break;
            }
        }

        cancel_token.cancel();
        let _ = handle.await;

        assert!(
            evicted,
            "Decay controller worker task should evict low score document"
        );
    }
}
