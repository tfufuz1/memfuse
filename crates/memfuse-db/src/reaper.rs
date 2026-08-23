use memfuse_core::tx_buffer::TxBuffer;
use std::sync::Arc;
use std::time::Duration;

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
                    let expired = buffer.reap_orphans();
                    if !expired.is_empty() {
                        tracing::warn!(
                            "Orphan reaper cleaned up {} expired transactions",
                            expired.len()
                        );
                    }
                    if let Err(err) = hnsw_index.check_connectivity() {
                        tracing::warn!(
                            error = %err,
                            "HNSW index degraded — consider calling rebuild()"
                        );
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
        let hnsw_index = Arc::new(memfuse_index::hnsw::HnswIndex::new(config));
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
}
