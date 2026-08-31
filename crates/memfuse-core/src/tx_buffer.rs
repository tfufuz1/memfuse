//! Transactional buffer for staging index operations.
//!
//! Sharded into sub-buffers to reduce lock contention.
//! Each shard is independently locked, allowing concurrent writers
//! to different transactions.
//!
//! # INVARIANT
//! To avoid deadlocks, callers must never acquire more than one shard lock simultaneously.
//! In multi-shard sweeps, `reap_orphans()` acquires all shards sequentially in ascending index
//! order (index 0 to N-1), acquiring and releasing each shard lock one at a time via `try_write()`.

// FILE-CONTEXT
// STAND: 2026-08-30T18:51:56Z (SESSION: e459bd5f)
// ZWECK: Shard-basierter Transaktionsbuffer für das Staging von 2-Phase-Commit Index-Operationen.
// INVARIANTEN: Shard-Isolation per TxId; Niemals zwei Shards gleichzeitig sperren (Deadlock-Prävention).
// HOTSPOTS: 110-380
// NICHT-OFFENSICHTLICH: Orphan Reaper führt getrennte try_write Locks pro Sharding-Index 0..N-1 durch.
// SIEHE AUCH: rules/tag_taxonomy.md, DECISIONS.md

// INVARIANT: Sharded Transaction Buffer für lock-freie Concurrency.

// FILE-CONTEXT
// STAND:       2026-08-29T15:22:34Z (SESSION: 2c814094)
// ZWECK:       Transaktion-Staging-Buffer zwischen Writes und WAL-Commit
// INVARIANTEN: Bounded capacity enforced (AGT-CORE-001), single shard lock acquired sequentially in index order
// HOTSPOTS:    TxBuffer::stage_insert(), TxBuffer::commit_tx(), reap_orphans()
// SIEHE AUCH:  crates/memfuse-core/AGENTS.md

use crate::error::{MemFuseError, Result};
use crate::types::{DocId, TxId};
use ahash::AHashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Default number of shards for the transaction buffer.
pub const DEFAULT_SHARD_COUNT: usize = 64;

/// Recommended maximum operations per single transaction to guard against memory exhaustion DoS.
// AI-TAG[SMELL][MINOR] RESOLVED: AGT-CORE-001 — Bounded staging capacity enforced (TS:2026-08-29T12:00:00Z) (SESSION: a3f29c1d)
pub const DEFAULT_MAX_OPS_PER_TX: usize = 10_000;

/// Configuration options for `TxBuffer`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxBufferConfig {
    /// Transaction timeout duration.
    pub tx_timeout: Duration,
    /// Maximum recommended active transactions.
    pub max_active_tx: usize,
    /// Maximale Anzahl von Staging-Operationen pro Transaktion.
    /// Verhindert OOM durch einzelne unbegrenzte Transaktionen.
    /// Default: 10_000 (großzügig, aber bounded).
    pub max_ops_per_tx: usize,
}

impl Default for TxBufferConfig {
    fn default() -> Self {
        Self {
            tx_timeout: Duration::from_secs(30),
            max_active_tx: 64,
            max_ops_per_tx: DEFAULT_MAX_OPS_PER_TX,
        }
    }
}

/// Operation to be executed in an index.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum IndexOp<T: Clone> {
    /// Insert a document with associated data.
    Insert {
        /// Document identifier to insert.
        doc_id: DocId,
        /// Associated payload or index data.
        data: T,
    },
    /// Delete a document.
    Delete {
        /// Document identifier to delete.
        doc_id: DocId,
        /// Optional payload associated with deletion.
        data: Option<T>,
    },
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
/// # INVARIANT
/// Each shard is protected by an independent `parking_lot::RwLock`.
/// Standard acquisition order: Read-lock for queries, Write-lock for mutations.
/// To avoid deadlocks, cross-shard operations must never acquire more than
/// one shard lock simultaneously. Operations that process all shards (like `reap_orphans()`)
/// MUST iterate over shards sequentially in ascending index order (from shard 0 to N-1)
/// and release each lock before acquiring the next.
#[derive(Debug)]
pub struct TxBuffer<T: Clone> {
    shards: Vec<RwLock<TxShard<T>>>,
    tx_timeout: Duration,
    config: TxBufferConfig,
}

impl<T: Clone> TxBuffer<T> {
    /// Creates a new buffer with default 64 shards and 30s timeout.
    pub fn new() -> Self {
        Self::new_with_config(DEFAULT_SHARD_COUNT, Duration::from_secs(30))
    }

    /// Creates a new buffer with custom settings.
    ///
    /// If `shard_count` is 0, it defaults to 1 to prevent division-by-zero
    /// in `shard_idx()` (§2 Zero-Panic-Gesetz).
    pub fn new_with_config(shard_count: usize, tx_timeout: Duration) -> Self {
        let config = TxBufferConfig {
            tx_timeout,
            ..Default::default()
        };
        Self::new_with_config_ext(shard_count, tx_timeout, config)
    }

    /// Creates a new buffer with explicit `TxBufferConfig`.
    pub fn new_with_config_ext(
        shard_count: usize,
        _tx_timeout: Duration,
        config: TxBufferConfig,
    ) -> Self {
        let shard_count = if shard_count == 0 { 1 } else { shard_count };
        let mut shards = Vec::with_capacity(shard_count);
        for _ in 0..shard_count {
            shards.push(RwLock::new(TxShard::new()));
        }
        Self {
            shards,
            tx_timeout: config.tx_timeout,
            config,
        }
    }

    #[inline]
    fn shard_idx(&self, tx: TxId) -> usize {
        // SAFETY: Modulo-Cast u64→usize (sicher wegen %-Operator)
        (tx.inner() % self.shards.len() as u64) as usize
    }

    /// Checks if the given transaction exists in the buffer.
    pub fn has_tx(&self, tx: TxId) -> bool {
        // SAFETY: Slice-Indexing — sicher weil shard_idx = modulo len()
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

    /// Stages an operation for the given transaction, checking bounded capacity.
    ///
    /// # BREAKING CHANGE
    /// Returns `Result<(), MemFuseError>` to enforce bounded capacity limits.
    pub fn stage(&self, tx: TxId, op: IndexOp<T>) -> Result<()> {
        self.stage_bounded(tx, op)
    }

    /// Stages an operation for the given transaction, enforcing `max_ops_per_tx` limit.
    pub fn stage_bounded(&self, tx: TxId, op: IndexOp<T>) -> Result<()> {
        let shard_idx = self.shard_idx(tx);
        let mut shard = self.shards[shard_idx].write();
        let entry = shard
            .ops
            .entry(tx)
            .or_insert_with(|| (Vec::with_capacity(16), Instant::now()));

        if entry.0.len() >= self.config.max_ops_per_tx {
            return Err(MemFuseError::Transaction(format!(
                "Transaction {} exceeded max staging capacity ({} ops)",
                tx, self.config.max_ops_per_tx
            )));
        }

        entry.0.push(op);
        Ok(())
    }

    /// Stages multiple operations for a transaction in a single shard lock acquisition.
    ///
    /// All `ops` must belong to the same transaction. Since `shard_idx` is a pure
    /// function of `tx.inner()`, all operations for a given `TxId` always land in
    /// the same shard — a single write-lock acquisition covers the entire batch.
    pub fn stage_many(&self, tx: TxId, ops: impl IntoIterator<Item = IndexOp<T>>) -> Result<()> {
        let shard_idx = self.shard_idx(tx);
        let mut shard = self.shards[shard_idx].write();
        let entry = shard
            .ops
            .entry(tx)
            .or_insert_with(|| (Vec::new(), Instant::now()));

        let ops_vec: Vec<_> = ops.into_iter().collect();
        if entry.0.len() + ops_vec.len() > self.config.max_ops_per_tx {
            return Err(MemFuseError::Transaction(format!(
                "Transaction {} exceeded max staging capacity ({} ops)",
                tx, self.config.max_ops_per_tx
            )));
        }

        entry.0.extend(ops_vec);
        Ok(())
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
    ///
    /// WARNING: This iterates over all shards with individual read locks.
    /// The result is NOT an atomic snapshot of the buffer's size.
    /// Do not use this as a strict control metric for backpressure.
    pub fn len(&self) -> usize {
        self.shards.iter().map(|s| s.read().ops.len()).sum()
    }

    /// Returns true if all shards are empty.
    ///
    /// WARNING: Like `len()`, this is not an atomic operation across all shards.
    pub fn is_empty(&self) -> bool {
        self.shards.iter().all(|s| s.read().ops.is_empty())
    }

    /// Cleans up expired transactions.
    /// Reaps expired orphan transactions.
    ///
    /// # INVARIANT
    /// Acquires shard locks sequentially in ascending index order (0 to N-1),
    /// dropping each lock before attempting the next to guarantee deadlock-free execution.
    pub fn reap_orphans(&self) -> Vec<TxId> {
        self.reap_orphans_bounded(usize::MAX)
    }

    /// Reaps up to `max` expired orphan transactions across shards.
    ///
    /// # INVARIANT
    /// Acquires shard locks sequentially in ascending index order (0 to N-1),
    /// dropping each lock before attempting the next to guarantee deadlock-free execution.
    pub fn reap_orphans_bounded(&self, max: usize) -> Vec<TxId> {
        let mut expired = Vec::new();
        for shard_lock in &self.shards {
            if expired.len() >= max {
                break;
            }
            if let Some(mut shard) = shard_lock.try_write() {
                shard.ops.retain(|tx, (_, created)| {
                    if expired.len() < max && created.elapsed() > self.tx_timeout {
                        expired.push(*tx);
                        false
                    } else {
                        true
                    }
                });
            }
            std::thread::yield_now();
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
    use proptest::{prop_assert, prop_assert_eq};
    use std::sync::Arc;

    #[test]
    fn test_txbuffer_respects_max_ops_limit() {
        let config = TxBufferConfig {
            max_ops_per_tx: 3,
            ..Default::default()
        };
        let buffer = TxBuffer::<String>::new_with_config_ext(64, Duration::from_secs(5), config);
        let tx = TxId::new(1);
        buffer.begin(tx);
        // 3 operations succeed
        for i in 0..3 {
            let res = buffer.stage_bounded(
                tx,
                IndexOp::Insert {
                    doc_id: DocId::new(i),
                    data: format!("data_{i}"),
                },
            );
            assert!(res.is_ok());
        }
        // 4th operation must fail
        let result = buffer.stage_bounded(
            tx,
            IndexOp::Insert {
                doc_id: DocId::new(99),
                data: "overflow".to_string(),
            },
        );
        assert!(matches!(result, Err(MemFuseError::Transaction(_))));
    }

    #[test]
    fn test_txbuffer_default_config_allows_normal_workload() {
        let buffer = TxBuffer::<String>::new();
        let tx = TxId::new(42);
        buffer.begin(tx);
        for i in 0..500 {
            let res = buffer.stage(
                tx,
                IndexOp::Insert {
                    doc_id: DocId::new(i),
                    data: format!("data_{i}"),
                },
            );
            assert!(res.is_ok());
        }
        assert_eq!(buffer.get_ops(tx).map(|v| v.len()), Some(500));
    }

    #[test]
    fn test_tx_buffer_stage_many_and_max_ops_constant() {
        let buffer = TxBuffer::<String>::new();
        let tx = TxId::new(42);
        assert_eq!(DEFAULT_MAX_OPS_PER_TX, 10_000);

        let ops = vec![
            IndexOp::Insert {
                doc_id: DocId::new(1),
                data: "op1".to_string(),
            },
            IndexOp::Insert {
                doc_id: DocId::new(2),
                data: "op2".to_string(),
            },
        ];

        assert!(buffer.stage_many(tx, ops).is_ok());
        assert!(buffer.has_tx(tx));
        let drained = buffer.drain(tx);
        assert_eq!(drained.len(), 2);
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_tx_buffer_stage_many_exceeds_capacity() {
        let config = TxBufferConfig {
            max_ops_per_tx: 2,
            ..Default::default()
        };
        let buffer = TxBuffer::<String>::new_with_config_ext(1, Duration::from_secs(5), config);
        let tx = TxId::new(10);

        let ops = vec![
            IndexOp::Insert {
                doc_id: DocId::new(1),
                data: "op1".to_string(),
            },
            IndexOp::Insert {
                doc_id: DocId::new(2),
                data: "op2".to_string(),
            },
            IndexOp::Insert {
                doc_id: DocId::new(3),
                data: "op3".to_string(),
            },
        ];

        let res = buffer.stage_many(tx, ops);
        assert!(matches!(res, Err(MemFuseError::Transaction(_))));
    }

    #[test]
    fn test_tx_buffer_reap_orphans_bounded_capping() {
        let buffer = TxBuffer::<String>::new_with_config(1, Duration::from_millis(5));

        buffer.begin(TxId::new(1));
        buffer.begin(TxId::new(2));
        buffer.begin(TxId::new(3));

        std::thread::sleep(Duration::from_millis(20));

        let reaped = buffer.reap_orphans_bounded(2);
        assert_eq!(reaped.len(), 2);
        assert_eq!(buffer.len(), 1);
    }

    #[test]
    fn test_tx_buffer_discard() {
        let buffer = TxBuffer::<String>::new_with_config(64, Duration::from_secs(30));
        let tx = TxId::new(1);

        let _ = buffer.stage(
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
                    let _ = buffer.stage(
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
            // INTENT: intentional expect in tests
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

    #[tokio::test]
    async fn test_has_tx_concurrent() {
        let buffer = Arc::new(TxBuffer::<String>::new());
        let num_tasks = 8;
        let mut handles = Vec::new();

        for i in 0..num_tasks {
            let buffer = buffer.clone();
            let tx = TxId::new(i as u64 + 100);
            handles.push(tokio::spawn(async move {
                buffer.begin(tx);
                let _ = buffer.stage(
                    tx,
                    IndexOp::Insert {
                        doc_id: DocId::new(i as u64),
                        data: format!("data_{i}"),
                    },
                );
                // Return tx ID for caller verification
                tx
            }));
        }

        let mut txs = Vec::new();
        for h in handles {
            let tx = h.await.expect("task failed"); // expect
            txs.push(tx);
        }

        // Verify has_tx returns true for all concurrently active transactions
        for &tx in &txs {
            assert!(buffer.has_tx(tx));
        }

        // Commit / drain all transactions and verify has_tx returns false
        for &tx in &txs {
            buffer.drain(tx);
            assert!(!buffer.has_tx(tx));
        }

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
        let _ = buffer.stage(
            tx,
            IndexOp::Insert {
                doc_id: DocId::new(1),
                data: "s".to_string(),
            },
        );
        assert!(buffer.validate_pending_ops(tx).is_ok());
    }

    #[test]
    fn test_index_op_helpers() {
        let op = IndexOp::Insert {
            doc_id: DocId::new(1),
            data: "d",
        };
        assert_eq!(op.doc_id(), DocId::new(1));

        let op2 = IndexOp::Delete::<String> {
            doc_id: DocId::new(2),
            data: None,
        };
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

    #[test]
    fn test_tx_buffer_zero_shards_defaults_to_one() {
        // FIND-COR-001: shard_count=0 must not panic (§2 Zero-Panic)
        let buffer = TxBuffer::<u8>::new_with_config(0, Duration::from_secs(1));
        let tx = TxId::new(42);
        buffer.begin(tx);
        let _ = buffer.stage(
            tx,
            IndexOp::Insert {
                doc_id: DocId::new(1),
                data: 0,
            },
        );
        let ops = buffer.drain(tx);
        assert_eq!(ops.len(), 1);
        assert!(buffer.is_empty());
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
                let _ = buffer.stage(tx, IndexOp::Insert { doc_id: DocId::new(id), data: id });
            }

            // 2. Verify each TX only has its own data
            for &id in &tx_ids {
                let tx = TxId::new(id);
                let ops = buffer.get_ops(tx).unwrap(); // unwrap
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

        #[test]
        fn prop_tx_buffer_stage_drain_stage_lifecycle(
            tx_id_raw in 0..u64::MAX,
            first_ops in proptest::collection::vec(0..100u64, 1..20),
            second_ops in proptest::collection::vec(0..100u64, 1..20)
        ) {
            let buffer = TxBuffer::<u64>::new();
            let tx = TxId::new(tx_id_raw);

            // 1. Stage first batch
            for &val in &first_ops {
                let _ = buffer.stage(tx, IndexOp::Insert { doc_id: DocId::new(val), data: val });
            }

            // 2. Drain and verify matching first batch
            let drained1 = buffer.drain(tx);
            prop_assert_eq!(drained1.len(), first_ops.len());
            for (idx, op) in drained1.into_iter().enumerate() {
                match op {
                    IndexOp::Insert { doc_id, data } => {
                        prop_assert_eq!(doc_id.inner(), first_ops[idx]);
                        prop_assert_eq!(data, first_ops[idx]);
                    }
                    _ => panic!("Expected Insert"),
                }
            }

            // 3. Verify buffer is clean for this tx
            prop_assert!(buffer.get_ops(tx).is_none());
            prop_assert!(!buffer.has_tx(tx));

            // 4. Stage second batch
            for &val in &second_ops {
                let _ = buffer.stage(tx, IndexOp::Insert { doc_id: DocId::new(val), data: val });
            }

            // 5. Drain and verify matching second batch exactly (no ghost leakage)
            let drained2 = buffer.drain(tx);
            prop_assert_eq!(drained2.len(), second_ops.len());
            for (idx, op) in drained2.into_iter().enumerate() {
                match op {
                    IndexOp::Insert { doc_id, data } => {
                        prop_assert_eq!(doc_id.inner(), second_ops[idx]);
                        prop_assert_eq!(data, second_ops[idx]);
                    }
                    _ => panic!("Expected Insert"),
                }
            }
            prop_assert!(buffer.is_empty());
        }

        #[test]
        fn prop_tx_buffer_partial_discard_isolation(
            tx_ids in proptest::collection::vec(0..u64::MAX, 2..40),
            discard_indices in proptest::collection::vec(0..100usize, 1..20)
        ) {
            let buffer = TxBuffer::<u64>::new();

            // Setup unique tx ids and stage one op each
            let mut unique_txs = tx_ids;
            unique_txs.sort_unstable();
            unique_txs.dedup();
            if unique_txs.len() < 2 {
                return Ok(()); // Skip trivial cases
            }

            for &id in &unique_txs {
                let tx = TxId::new(id);
                let _ = buffer.stage(tx, IndexOp::Insert { doc_id: DocId::new(id), data: id });
            }

            // Determine which to discard
            let mut to_discard = std::collections::HashSet::new();
            for seed in discard_indices {
                let idx = seed % unique_txs.len();
                to_discard.insert(unique_txs[idx]);
            }

            // Discard selected
            for &id in &to_discard {
                buffer.discard(TxId::new(id));
            }

            // Verify isolated status: discarded are gone, others remain intact
            for &id in &unique_txs {
                let tx = TxId::new(id);
                if to_discard.contains(&id) {
                    prop_assert!(!buffer.has_tx(tx));
                    prop_assert!(buffer.get_ops(tx).is_none());
                } else {
                    prop_assert!(buffer.has_tx(tx));
                    let ops = buffer.get_ops(tx).unwrap(); // unwrap
                    prop_assert_eq!(ops.len(), 1);
                    match &ops[0] {
                        IndexOp::Insert { doc_id, data } => {
                            prop_assert_eq!(doc_id.inner(), id);
                            prop_assert_eq!(*data, id);
                        }
                        _ => panic!("Expected Insert"),
                    }
                }
            }
        }
    }
}
