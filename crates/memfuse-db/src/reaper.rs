// FILE-CONTEXT
// ZWECK: Hintergrund-Reaper-Tasks zur TTL-Löschung und Bereinigung verwaister Transaktionen (Orphan Reaper).
// INVARIANTEN: Geordnete Abschaltung via CancellationToken; Beschränkung der pro Tick verarbeiteten Elemente.
// NICHT-OFFENSICHTLICH: Orphan Reaper triggert bei HNSW-Indextrennung automatischen Rebuild mit Timeout.
// STAND: TS:2026-08-29T17:22:29Z (SESSION: 0dcb9f3b)

use crate::collection::Collection;
use memfuse_core::traits::StorageEngine;
use memfuse_core::tx_buffer::TxBuffer;
use std::sync::Arc;
use std::time::Duration;

/// Maximum number of orphan transactions processed in a single reaper tick
/// to avoid starving foreground operations.
pub const MAX_ORPHANS_PER_TICK: usize = 100;

/// Maximum number of expired documents processed in a single expiry reaper tick.
pub const MAX_EXPIRED_PER_TICK: usize = 100;

/// Starts a background task to periodically clean up expired documents with TTL.
pub fn start_expiry_reaper<S: StorageEngine>(
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
            "Expiry reaper task started"
        );
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    match collection.reap_expired_documents(MAX_EXPIRED_PER_TICK).await {
                        Ok(reaped) if reaped > 0 => {
                            tracing::info!(
                                collection = %collection.name(),
                                reaped = reaped,
                                "Expiry reaper cleaned up expired documents"
                            );
                        }
                        Ok(_) => {}
                        Err(err) => {
                            tracing::error!(
                                collection = %collection.name(),
                                error = %err,
                                "Error during expiry reaper execution"
                            );
                        }
                    }
                }
                _ = cancel_token.cancelled() => {
                    tracing::info!(
                        collection = %collection.name(),
                        "Expiry reaper task shutting down via token"
                    );
                    break;
                }
            }
        }
    })
}

/// Starts a background task to periodically clean up orphan transactions.
///
/// This reaper handles the cleanup of transactions that have exceeded their
/// configured timeout without being committed or rolled back.
pub fn start_orphan_reaper<T: Clone + Send + Sync + 'static>(
    buffer: Arc<TxBuffer<T>>,
    hnsw_index: Arc<memfuse_index::hnsw::HnswIndex>,
    interval: Duration,
    cancel_token: tokio_util::sync::CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        tracing::info!(
            "Orphan reaper started (timeout: {:?}, interval: {:?})",
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
                            "Orphan reaper cleaned up {} expired transactions",
                            expired.len()
                        );
                    }
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
                    tracing::info!("Orphan reaper shutting down via token");
                    break;
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use memfuse_core::tx_buffer::IndexOp;
    use memfuse_core::types::{DocId, TxId};
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_expiry_reaper_task_cleans_documents() {
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
        let handle =
            start_expiry_reaper(col.clone(), Duration::from_millis(10), cancel_token.clone());

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

        assert!(cleaned, "Expiry reaper task should delete expired document");
    }

    #[tokio::test]
    async fn test_orphan_reaper_removes_expired() {
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
        let _reaper = start_orphan_reaper(
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
            "Expired transaction should have been reaped within 500ms"
        );
    }

    #[tokio::test]
    async fn reaper_deletes_expired_documents() {
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

        col.trigger_reaper().await.unwrap(); // unwrap
        let result = col.get("doc1").await.unwrap(); // unwrap
        assert!(result.is_none(), "Expired document must be deleted");
    }

    #[tokio::test]
    async fn test_reaper_immediate_cancellation() {
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

        let handle = start_expiry_reaper(col, Duration::from_secs(60), cancel_token);
        let res = handle.await;
        assert!(res.is_ok(), "Task should exit cleanly upon cancellation");
    }
}
