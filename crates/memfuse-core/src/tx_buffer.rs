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

    /// Returns the transaction timeout.
    pub fn tx_timeout(&self) -> Duration {
        self.tx_timeout
    }
}

impl<T: Clone> Default for TxBuffer<T> {
    fn default() -> Self {
        Self::new()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use proptest::{prop_assert, prop_assert_eq};

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

    #[test]
    fn test_concurrent_stage_no_data_loss() {
        let buffer = Arc::new(TxBuffer::<usize>::new_with_config(
            64,
            Duration::from_secs(30),
        ));
        let num_tx = 100;
        let ops_per_tx = 100;

        let mut handles = Vec::new();
        for t in 0..num_tx {
            let buffer = buffer.clone();
            handles.push(std::thread::spawn(move || {
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
            h.join().expect("task panicked"); // #[cfg(test)]
        }

        assert_eq!(buffer.len(), num_tx);
        for t in 0..num_tx {
            let tx = TxId::new(t as u64);
            let ops = buffer.drain(tx);
            assert_eq!(ops.len(), ops_per_tx);
        }
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_tx_buffer_reap_orphans() {
        let buffer = TxBuffer::<String>::new_with_config(1, Duration::from_millis(10));
        let tx = TxId::new(1);
        buffer.begin(tx);
        
        // Wait for timeout
        std::thread::sleep(Duration::from_millis(20));
        
        let expired = buffer.reap_orphans();
        assert_eq!(expired, vec![tx]);
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_tx_buffer_validate_pending_ops() {
        let buffer = TxBuffer::<String>::new();
        let tx = TxId::new(1);
        
        // No tx yet
        assert!(buffer.validate_pending_ops(tx).is_ok());
        
        // Registered but empty
        buffer.begin(tx);
        assert!(buffer.validate_pending_ops(tx).is_err());
        
        // With ops
        buffer.stage(tx, IndexOp::Insert { doc_id: DocId::new(1), data: "s".to_string() });
        assert!(buffer.validate_pending_ops(tx).is_ok());
    }

    #[test]
    fn test_index_op_helpers() {
        let op = IndexOp::Insert { doc_id: DocId::new(1), data: "d" };
        assert_eq!(op.doc_id(), DocId::new(1));
        
        let op2 = IndexOp::Delete::<String> { doc_id: DocId::new(2), data: None };
        assert_eq!(op2.doc_id(), DocId::new(2));
    }

    #[test]
    fn test_tx_buffer_config() {
        let timeout = Duration::from_secs(100);
        let buffer = TxBuffer::<u8>::new_with_config(4, timeout);
        assert_eq!(buffer.tx_timeout(), timeout);
        // Ensure we can use all shards
        for i in 0..10 {
            buffer.begin(TxId::new(i));
        }
    }

    proptest::proptest! {
        #[test]
        fn prop_tx_buffer_isolation(
            tx_ids in proptest::collection::vec(0..u64::MAX, 1..50),
            shard_count in 1..256usize
        ) {
            let buffer = TxBuffer::<u64>::new_with_config(shard_count, Duration::from_secs(60));
            
            // 1. Stage values for all unique TXs
            for &id in &tx_ids {
                let tx = TxId::new(id);
                buffer.stage(tx, IndexOp::Insert { doc_id: DocId::new(id), data: id });
            }

            // 2. Verify each TX only has its own data
            for &id in &tx_ids {
                let tx = TxId::new(id);
                let ops = buffer.get_ops(tx).unwrap();
                for op in ops {
                    match op {
                        IndexOp::Insert { doc_id, data } => {
                            prop_assert_eq!(doc_id.inner(), id);
                            prop_assert_eq!(data, id);
                        },
                        _ => panic!("Unexpected op"),
                    }
                }
            }

            // 3. Drain and verify emptiness
            for &id in &tx_ids {
                let tx = TxId::new(id);
                buffer.drain(tx);
            }
            prop_assert!(buffer.is_empty());
        }

        #[test]
        fn prop_tx_buffer_reap_is_complete(
            tx_count in 1..100usize,
            timeout_ms in 1..100u64
        ) {
            let buffer = TxBuffer::<u8>::new_with_config(16, Duration::from_millis(timeout_ms));
            
            for i in 0..tx_count {
                buffer.begin(TxId::new(i as u64));
            }

            // Wait double the timeout
            std::thread::sleep(Duration::from_millis(timeout_ms * 2));
            
            let reaped = buffer.reap_orphans();
            prop_assert_eq!(reaped.len(), tx_count);
            prop_assert!(buffer.is_empty());
        }
    }
}
