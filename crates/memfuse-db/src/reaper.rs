use memfuse_core::tx_buffer::TxBuffer;
use std::sync::Arc;
use std::time::Duration;

/// Starts a background task to periodically clean up orphan transactions.
///
/// This reaper handles the cleanup of transactions that have exceeded their
/// configured timeout without being committed or rolled back.
pub fn start_orphan_reaper<T: Clone + Send + Sync + 'static>(
    buffer: Arc<TxBuffer<T>>,
    interval: Duration,
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
            ticker.tick().await;
            let expired = buffer.reap_orphans();
            if !expired.is_empty() {
                tracing::warn!(
                    "Orphan reaper cleaned up {} expired transactions",
                    expired.len()
                );
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

        let _reaper = start_orphan_reaper(buffer.clone(), Duration::from_millis(10));
        assert!(buffer.has_tx(tx1));
        sleep(Duration::from_millis(100)).await;
        assert!(!buffer.has_tx(tx1));
    }
}
