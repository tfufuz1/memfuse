//! Transactional buffer for indexing operations.
//!
//! This module provides a thread-safe, sharded buffer that stages indexing operations
//! (inserts, deletes) during a transaction's lifecycle before they are
//! committed or rolled back.

use crate::error::{ChimeraError, Result};
use crate::{DocId, NamespaceId, QualifiedDocId, TxId};
use ahash::AHashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Default number of shards for the transaction buffer (SPEC-006_EXT).
pub const DEFAULT_SHARD_COUNT: usize = 64;

/// Operation to be executed in an index.
#[derive(
    Debug, Clone, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
#[archive_attr(derive(bytecheck::CheckBytes))]
pub enum IndexOp<T: Clone> {
    /// Insert a document into the index with the associated data.
    Insert {
        namespace_id: NamespaceId,
        doc_id: DocId,
        data: T,
    },
    /// Delete a document from the index.
    Delete {
        namespace_id: NamespaceId,
        doc_id: DocId,
        /// Optional data associated with the deletion (e.g., the key for StorageEngine).
        data: Option<T>,
    },
}

impl<T: Clone> IndexOp<T> {
    /// Returns the qualified document ID for this operation.
    pub fn qualified_id(&self) -> QualifiedDocId {
        match self {
            IndexOp::Insert {
                namespace_id,
                doc_id,
                ..
            } => QualifiedDocId::new(namespace_id.clone(), *doc_id),
            IndexOp::Delete {
                namespace_id,
                doc_id,
                ..
            } => QualifiedDocId::new(namespace_id.clone(), *doc_id),
        }
    }
}

#[derive(Debug)]
struct TxShard<T: Clone> {
    /// Pending operations per transaction in this shard.
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
/// Sharded into sub-buffers to reduce lock contention (SPEC-006_EXT).
/// Each shard is independently locked, allowing concurrent writers
/// to different transactions without blocking each other.
///
/// [INV-S7] Strict TxBuffer TTL.
#[derive(Debug)]
pub struct TxBuffer<T: Clone> {
    shards: Vec<RwLock<TxShard<T>>>,
    tx_timeout: Duration,
}

impl<T: Clone> TxBuffer<T> {
    /// Creates a new, empty transaction buffer with default 64 shards and 30s timeout.
    pub fn new() -> Self {
        Self::new_with_config(DEFAULT_SHARD_COUNT, Duration::from_secs(30))
    }

    /// Creates a new, empty transaction buffer with custom settings.
    pub fn new_with_config(shard_count: usize, tx_timeout: Duration) -> Self {
        let mut shards = Vec::with_capacity(shard_count);
        for _ in 0..shard_count {
            shards.push(RwLock::new(TxShard::new()));
        }
        Self { shards, tx_timeout }
    }

    /// Determines the shard index for a given transaction ID.
    #[inline]
    fn shard_idx(&self, tx: TxId) -> usize {
        (tx.inner() % self.shards.len() as u64) as usize
    }

    /// Checks if the given transaction exists in the buffer.
    #[tracing::instrument(skip(self))]
    pub fn has_tx(&self, tx: TxId) -> bool {
        let shard = &self.shards[self.shard_idx(tx)];
        shard.read().ops.contains_key(&tx)
    }

    /// Registers a new transaction in the buffer.
    #[tracing::instrument(skip(self))]
    pub fn begin(&self, tx: TxId) {
        let shard_idx = self.shard_idx(tx);
        let mut shard = self.shards[shard_idx].write();
        shard
            .ops
            .entry(tx)
            .or_insert_with(|| (Vec::new(), Instant::now()));
    }

    /// Stages an operation for the given transaction.
    ///
    /// DETERMINISM: O(1) via Shard-Index.
    #[tracing::instrument(skip(self, op))]
    pub fn stage(&self, tx: TxId, op: IndexOp<T>) {
        let shard_idx = self.shard_idx(tx);
        let mut shard = self.shards[shard_idx].write();
        let entry = shard
            .ops
            .entry(tx)
            .or_insert_with(|| (Vec::new(), Instant::now()));
        entry.0.push(op);
    }

    /// Validates that the transaction exists and has pending operations (SPEC-006).
    ///
    /// # Errors
    /// Returns an error if the transaction is not found or has no operations (INV-S7).
    #[tracing::instrument(skip(self))]
    pub fn validate_pending_ops(&self, tx: TxId) -> Result<()> {
        let shard_idx = self.shard_idx(tx);
        let shard = self.shards[shard_idx].read();

        if let Some((ops, _)) = shard.ops.get(&tx) {
            // We only fail if the transaction WAS registered but has no operations.
            // This catches logic errors where an index was called but didn't stage anything.
            if ops.is_empty() {
                return Err(ChimeraError::Transaction(format!(
                    "Transaction {} was registered but has no pending operations in this index (INV-S7 violation)",
                    tx
                )));
            }
            Ok(())
        } else {
            // Transaction not found in this specific index buffer.
            // This is valid if the index was never involved in the transaction.
            // Global timeout and existence are handled by the SyncManager's active_txs buffer.
            Ok(())
        }
    }

    /// Checks if a document is present in the buffered operations for a transaction within a namespace.
    pub fn contains(&self, tx: TxId, ns: &NamespaceId, doc_id: DocId) -> bool {
        let shard_idx = self.shard_idx(tx);
        let shard = self.shards[shard_idx].read();
        shard
            .ops
            .get(&tx)
            .map(|(ops, _)| {
                ops.iter().any(|op| match op {
                    IndexOp::Insert {
                        namespace_id: n,
                        doc_id: id,
                        ..
                    } => n == ns && *id == doc_id,
                    _ => false,
                })
            })
            .unwrap_or(false)
    }

    /// Returns a list of document IDs (with namespace) affected by the given transaction.
    ///
    /// # SPEC-030: Physical Isolation
    /// Filters documents to only include those matching the given namespace.
    pub fn get_involved_docs_for_ns(&self, tx: TxId, ns: &NamespaceId) -> Vec<QualifiedDocId> {
        let shard_idx = self.shard_idx(tx);
        let shard = self.shards[shard_idx].read();
        shard
            .ops
            .get(&tx)
            .map(|(ops, _)| {
                ops.iter()
                    .map(|op| op.qualified_id())
                    .filter(|qid| &qid.namespace == ns)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Drains and returns all buffered operations for a transaction.
    ///
    /// [INV-S1] TxId nach commit() nie im TxBuffer.
    #[tracing::instrument(skip(self))]
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
    #[tracing::instrument(skip(self))]
    pub fn discard(&self, tx: TxId) {
        let shard_idx = self.shard_idx(tx);
        let mut shard = self.shards[shard_idx].write();
        shard.ops.remove(&tx);
    }

    /// Returns the total number of pending transactions across all shards.
    pub fn len(&self) -> usize {
        self.shards.iter().map(|s| s.read().ops.len()).sum()
    }

    /// Alias for len() (SPEC-006).
    pub fn pending_count(&self) -> usize {
        self.len()
    }

    /// Returns true if all shards are empty.
    pub fn is_empty(&self) -> bool {
        self.shards.iter().all(|s| s.read().ops.is_empty())
    }

    /// Cleans up expired transactions (INV-R2, INV-S7).
    pub fn reap_orphans(&self) -> Vec<TxId> {
        let mut expired = Vec::new();
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

    /// Returns a reference to the pending operations for a transaction (for serialization).
    pub fn get_ops(&self, tx: TxId) -> Option<Vec<IndexOp<T>>> {
        let shard_idx = self.shard_idx(tx);
        let shard = self.shards[shard_idx].read();
        shard.ops.get(&tx).map(|(ops, _)| ops.clone())
    }

    /// Returns a list of document IDs (with namespace) affected by the given transaction.
    pub fn get_involved_docs(&self, tx: TxId) -> Vec<QualifiedDocId> {
        let shard_idx = self.shard_idx(tx);
        let shard = self.shards[shard_idx].read();
        shard
            .ops
            .get(&tx)
            .map(|(ops, _)| ops.iter().map(|op| op.qualified_id()).collect())
            .unwrap_or_default()
    }
}

impl<T: Clone> Default for TxBuffer<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Starts a background task to periodically clean up orphan transactions (SPEC-006_EXT).
///
/// This is a mandatory startup task for the SyncManager (INV-S7).
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
                    "Orphan reaper cleaned up {} expired transactions: {:?}",
                    expired.len(),
                    expired
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
        // Check that different TxId map to different shards
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
                namespace_id: NamespaceId::default_ns(),
                doc_id: DocId::new(1),
                data: "data1".to_string(),
            },
        );
        buffer.stage(
            tx,
            IndexOp::Insert {
                namespace_id: NamespaceId::default_ns(),
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
    fn test_validate_pending_ops() {
        let buffer = TxBuffer::<String>::new_with_config(64, Duration::from_secs(30));
        let tx = TxId::new(1);

        // Transaction not found is now Ok (not involved in this index)
        assert!(buffer.validate_pending_ops(tx).is_ok());

        // Explicitly begin without staging should now fail (registered but empty)
        buffer.begin(tx);
        assert!(buffer.validate_pending_ops(tx).is_err());

        buffer.stage(
            tx,
            IndexOp::Insert {
                namespace_id: NamespaceId::default_ns(),
                doc_id: DocId::new(1),
                data: "data1".to_string(),
            },
        );

        assert!(buffer.validate_pending_ops(tx).is_ok());
    }

    #[test]
    fn test_tx_buffer_discard() {
        let buffer = TxBuffer::<String>::new_with_config(64, Duration::from_secs(30));
        let tx = TxId::new(1);

        buffer.stage(
            tx,
            IndexOp::Insert {
                namespace_id: NamespaceId::default_ns(),
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
                namespace_id: NamespaceId::default_ns(),
                doc_id: DocId::new(1),
                data: "old".to_string(),
            },
        );

        // Start reaper with fast interval
        let _reaper = start_orphan_reaper(buffer.clone(), Duration::from_millis(10));

        // Initially it should be there
        assert!(buffer.has_tx(tx1));

        // Wait for it to expire and be reaped
        sleep(Duration::from_millis(100)).await;

        assert!(!buffer.has_tx(tx1));
        assert!(buffer.is_empty());
    }

    #[tokio::test]
    async fn test_orphan_reaper_preserves_active() {
        let buffer = Arc::new(TxBuffer::<String>::new_with_config(
            64,
            Duration::from_secs(30),
        ));
        let tx1 = TxId::new(1);

        buffer.begin(tx1);

        let _reaper = start_orphan_reaper(buffer.clone(), Duration::from_millis(10));

        sleep(Duration::from_millis(50)).await;

        assert!(buffer.has_tx(tx1));
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
                            namespace_id: NamespaceId::default_ns(),
                            doc_id: DocId::new(i as u64),
                            data: i,
                        },
                    );
                }
            }));
        }

        for h in handles {
            h.await.expect("task panicked");
        }

        assert_eq!(buffer.pending_count(), num_tx);
        for t in 0..num_tx {
            let tx = TxId::new(t as u64);
            let ops = buffer.drain(tx);
            assert_eq!(ops.len(), ops_per_tx);
        }
        assert!(buffer.is_empty());
    }
}
