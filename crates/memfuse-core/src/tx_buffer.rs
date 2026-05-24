//! Transactional buffer for staging index operations.
//!
//! Sharded into sub-buffers to reduce lock contention.
//! Each shard is independently locked, allowing concurrent writers
//! to different transactions.

// ANCHOR:ARCH:TXBUF-001 — Sharded Transaction Buffer für lock-freie Concurrency.
// WP:WP-0.0 PRIO:1 NEEDS:NONE
// AGENT:01 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-05 DEADLINE:NONE
// DESIGN: 64 Shards → TxId % 64. LIFECYCLE: stage() → drain()/discard().

use crate::error::{MemFuseError, Result};
use crate::types::{DocId, TxId};
use ahash::AHashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Default number of shards for the transaction buffer.
pub const DEFAULT_SHARD_COUNT: usize = 64;

/// Operation to be executed in an index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IndexOp<T: Clone> {
    /// Insert a document with associated data.
    Insert { doc_id: DocId, data: T },
    /// Delete a document.
    Delete { doc_id: DocId, data: Option<T> },
}

impl<T: Clone> IndexOp<T> {
    /// Returns the document ID for this operation.
    pub fn doc_id(&self) -> DocId {
        match self {
            IndexOp::Insert { doc_id, .. } => *doc_id,
            IndexOp::Delete { doc_id, .. } => *doc_id,
        }
    }
}

#[derive(Debug)]
struct TxShard<T: Clone> {
    ops: AHashMap<TxId, (Vec<IndexOp<T>>, Instant)>,
}

impl<T: Clone> TxShard<T> {
    fn new() -> Self {
        Self {
            ops: AHashMap::new(),
        }
    }
}

/// Buffers index operations until commit or rollback.
///
/// Sharded into sub-buffers to reduce lock contention.
///
/// ### Locking Strategy
/// Each shard is protected by an independent `parking_lot::RwLock`.
/// Standard acquisition order: Read-lock for queries, Write-lock for mutations.
/// To avoid deadlocks, cross-shard operations must never acquire more than
/// one shard lock simultaneously.
#[derive(Debug)]
pub struct TxBuffer<T: Clone> {
    shards: Vec<RwLock<TxShard<T>>>,
    tx_timeout: Duration,
}

impl<T: Clone> TxBuffer<T> {
    /// Creates a new buffer with default 64 shards and 30s timeout.
    pub fn new() -> Self {
        Self::new_with_config(DEFAULT_SHARD_COUNT, Duration::from_secs(30))
    }

    /// Creates a new buffer with custom settings.
    pub fn new_with_config(shard_count: usize, tx_timeout: Duration) -> Self {
        let mut shards = Vec::with_capacity(shard_count);
        for _ in 0..shard_count {
            shards.push(RwLock::new(TxShard::new()));
        }
        Self { shards, tx_timeout }
    }

    #[inline]
    fn shard_idx(&self, tx: TxId) -> usize {
        // ANCHOR:SEC:CAST-001 — Modulo-Cast u64→usize (sicher wegen %-Operator)
        // WP:WP-0.0 PRIO:5 NEEDS:NONE
        // AGENT:10 DATE:2026-05-09 STATUS:DONE
        // CREATED:2026-05-09 DEADLINE:NONE
        (tx.inner() % self.shards.len() as u64) as usize
    }

    /// Checks if the given transaction exists in the buffer.
    pub fn has_tx(&self, tx: TxId) -> bool {
        // ANCHOR:SEC:SLICE-001 — Slice-Indexing — sicher weil shard_idx = modulo len()
        // WP:WP-0.0 PRIO:5 NEEDS:NONE
        // AGENT:10 DATE:2026-05-09 STATUS:DONE
        // CREATED:2026-05-09 DEADLINE:NONE
        let shard = &self.shards[self.shard_idx(tx)];
        shard.read().ops.contains_key(&tx)
    }

    /// Registers a new transaction in the buffer.
    pub fn begin(&self, tx: TxId) {
        let shard_idx = self.shard_idx(tx);
        let mut shard = self.shards[shard_idx].write();
        shard
            .ops
            .entry(tx)
            .or_insert_with(|| (Vec::with_capacity(16), Instant::now()));
    }

    /// Stages an operation for the given transaction.
    ///
    /// If the transaction has not been explicitly started with `begin`,
    /// it will be implicitly created on the first `stage` call.
    pub fn stage(&self, tx: TxId, op: IndexOp<T>) {
        let shard_idx = self.shard_idx(tx);
        let mut shard = self.shards[shard_idx].write();
        let entry = shard
            .ops
            .entry(tx)
            .or_insert_with(|| (Vec::with_capacity(16), Instant::now()));
        entry.0.push(op);
    }

    /// Validates that the transaction has pending operations.
    pub fn validate_pending_ops(&self, tx: TxId) -> Result<()> {
        let shard_idx = self.shard_idx(tx);
        let shard = self.shards[shard_idx].read();

        if let Some((ops, _)) = shard.ops.get(&tx) {
            if ops.is_empty() {
                return Err(MemFuseError::Transaction(format!(
                    "Transaction {} was registered but has no pending operations",
                    tx
                )));
            }
        }
        Ok(())
    }

    /// Drains and returns all buffered operations for a transaction.
    ///
    /// Returns an empty vector if the transaction does not exist or has no operations.
    /// This operation is atomic per shard.
    pub fn drain(&self, tx: TxId) -> Vec<IndexOp<T>> {
        let shard_idx = self.shard_idx(tx);
        let mut shard = self.shards[shard_idx].write();
        shard
            .ops
            .remove(&tx)
            .map(|(ops, _)| ops)
            .unwrap_or_default()
    }

    /// Discards all buffered operations for a transaction.
    pub fn discard(&self, tx: TxId) {
        let shard_idx = self.shard_idx(tx);
        let mut shard = self.shards[shard_idx].write();
        shard.ops.remove(&tx);
    }

    /// Returns the total number of pending transactions.
    pub fn len(&self) -> usize {
        self.shards.iter().map(|s| s.read().ops.len()).sum()
    }

    /// Returns true if all shards are empty.
    pub fn is_empty(&self) -> bool {
        self.shards.iter().all(|s| s.read().ops.is_empty())
    }

    /// Cleans up expired transactions.
    pub fn reap_orphans(&self) -> Vec<TxId> {
        let mut expired = Vec::with_capacity(self.len());
        for shard_lock in &self.shards {
            let mut shard = shard_lock.write();
            shard.ops.retain(|tx, (_, created)| {
                if created.elapsed() > self.tx_timeout {
                    expired.push(*tx);
                    false
                } else {
                    true
                }
            });
        }
        expired
    }

    /// Returns a clone of the pending operations for a transaction.
    pub fn get_ops(&self, tx: TxId) -> Option<Vec<IndexOp<T>>> {
        let shard_idx = self.shard_idx(tx);
        let shard = self.shards[shard_idx].read();
        shard.ops.get(&tx).map(|(ops, _)| ops.clone())
    }
}

impl<T: Clone> Default for TxBuffer<T> {
    fn default() -> Self {
        Self::new()
    }
}

// ANCHOR:ARCH:REAPER-001 — Background Tokio-Task für verwaiste Transaktionen.
// WP:WP-0.0 PRIO:3 NEEDS:NONE
// AGENT:01 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-05 DEADLINE:NONE
// WARNUNG: Endlos-Loop — Tokio runtime drop killt den Task (akzeptiert).
/// Starts a background task to periodically clean up orphan transactions.
pub fn start_orphan_reaper<T: Clone + Send + Sync + 'static>(
    buffer: Arc<TxBuffer<T>>,
    interval: Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        tracing::info!(
            "Orphan reaper started (timeout: {:?}, interval: {:?})",
            buffer.tx_timeout,
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
    use std::time::Duration;
    use tokio::time::sleep;

    #[test]
    fn test_sharding_distributes_evenly() {
        let buffer = TxBuffer::<String>::new_with_config(64, Duration::from_secs(30));
        let shard0 = buffer.shard_idx(TxId::new(0));
        let shard1 = buffer.shard_idx(TxId::new(1));
        let shard64 = buffer.shard_idx(TxId::new(64));

        assert_eq!(shard0, 0);
        assert_eq!(shard1, 1);
        assert_eq!(shard64, 0);
    }

    #[test]
    fn test_tx_buffer_stage_drain() {
        let buffer = TxBuffer::<String>::new_with_config(64, Duration::from_secs(30));
        let tx = TxId::new(1);

        buffer.stage(
            tx,
            IndexOp::Insert {
                doc_id: DocId::new(1),
                data: "data1".to_string(),
            },
        );
        buffer.stage(
            tx,
            IndexOp::Insert {
                doc_id: DocId::new(2),
                data: "data2".to_string(),
            },
        );

        assert!(!buffer.is_empty());

        let ops = buffer.drain(tx);
        assert_eq!(ops.len(), 2);
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_tx_buffer_discard() {
        let buffer = TxBuffer::<String>::new_with_config(64, Duration::from_secs(30));
        let tx = TxId::new(1);

        buffer.stage(
            tx,
            IndexOp::Insert {
                doc_id: DocId::new(1),
                data: "data1".to_string(),
            },
        );
        buffer.discard(tx);
        assert!(buffer.is_empty());
    }

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

        // Poll for up to 2s to avoid flakiness
        let start = Instant::now();
        let mut removed = false;
        while start.elapsed() < Duration::from_secs(2) {
            if !buffer.has_tx(tx1) {
                removed = true;
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }

        assert!(removed, "Transaction should have been reaped");
    }

    #[tokio::test]
    async fn test_concurrent_stage_no_data_loss() {
        let buffer = Arc::new(TxBuffer::<usize>::new_with_config(
            64,
            Duration::from_secs(30),
        ));
        let num_tx = 100;
        let ops_per_tx = 100;

        let mut handles = Vec::new();
        for t in 0..num_tx {
            let buffer = buffer.clone();
            handles.push(tokio::spawn(async move {
                let tx = TxId::new(t as u64);
                buffer.begin(tx);
                for i in 0..ops_per_tx {
                    buffer.stage(
                        tx,
                        IndexOp::Insert {
                            doc_id: DocId::new(i as u64),
                            data: i,
                        },
                    );
                }
            }));
        }

        for h in handles {
            // ANCHOR:DEBT:TXBUF-002 — intentional expect in tests
            h.await.expect("task panicked"); // expect #[cfg(test)] // #[cfg(test)]
        }

        assert_eq!(buffer.len(), num_tx);
        for t in 0..num_tx {
            let tx = TxId::new(t as u64);
            let ops = buffer.drain(tx);
            assert_eq!(ops.len(), ops_per_tx);
        }
        assert!(buffer.is_empty());
    }
}
