use memfuse_core::tx_buffer::TxBuffer;
use std::sync::Arc;
use std::time::Duration;

/// Maximum number of orphan transactions processed in a single reaper tick
/// to avoid starving foreground operations.
pub const MAX_ORPHANS_PER_TICK: usize = 100;

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
    async fn test_orphan_reaper_removes_expired() {
        let buffer = Arc::new(TxBuffer::<String>::new_with_config(
            64,
            Duration::from_millis(50),
        ));
        let tx1 = TxId::new(1);

        buffer.begin(tx1);
        buffer.stage(
            tx1,
            IndexOp::Insert {
                doc_id: DocId::new(1),
                data: "old".to_string(),
            },
        );

        let cancel_token = tokio_util::sync::CancellationToken::new();
        let config = memfuse_index::hnsw::HnswConfig::default();
        let hnsw_index = Arc::new(memfuse_index::hnsw::HnswIndex::try_new(config).unwrap());
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

        let dir = tempdir().unwrap();
        let lsm_config = memfuse_store::LsmConfig {
            path: dir.path().to_path_buf(),
            ..Default::default()
        };
        let storage = Arc::new(LsmStorage::new(lsm_config).await.unwrap());
        let index = Arc::new(
            HnswIndex::try_new(memfuse_index::HnswConfig {
                dimension: 4,
                ..Default::default()
            })
            .unwrap(),
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
            .unwrap()
            .as_millis() as u64;

        col.insert(
            "doc1",
            &vec,
            Some(json!({"created_at_ms": now_ms - 100, "ttl_ms": 50})),
        )
        .await
        .unwrap();

        col.trigger_reaper().await.unwrap();
        let result = col.get("doc1").await.unwrap();
        assert!(result.is_none(), "Expired document must be deleted");
    }
}
