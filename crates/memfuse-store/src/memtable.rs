//! In-memory sorted MemTable for the LSM-Tree with MVCC support.
//!
//! Entries are sharded across `SHARD_COUNT` independent `BTreeMap` partitions,
//! each protected by its own `parking_lot::RwLock`. This reduces write-lock
//! contention when multiple coroutines insert concurrently (e.g. the 8-way
//! `buffer_unordered` ingestion pipeline).
//!
//! The shard for a given key is selected deterministically via the first byte
//! of the key modulo `SHARD_COUNT`. All MemFuse keys carry a fixed-length
//! namespace prefix, so the first byte is always present.
//!
//! Within each shard, each key maps to a versioned list of values, enabling
//! Snapshot Isolation through point-in-time reads.

// INVARIANT: In-Memory Sortierter Puffer (hot writes), sharded for concurrency.
// AI-NOTE: Sharding pattern mirrors memfuse-core::TxBuffer<T> (ADR-implicit).
//          Key difference: TxBuffer shards by TxId, MemTable shards by key bytes.

use bytes::Bytes;
use parking_lot::RwLock;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

type SequenceNumber = u64;
type TransactionId = u64;
type MemTableEntry = (SequenceNumber, Bytes, TransactionId);
type MemTableMap = BTreeMap<Bytes, Vec<MemTableEntry>>;

/// Number of independent shards. 16 provides sufficient parallelism for the
/// 8-concurrent-embed pipeline while keeping memory overhead negligible.
/// Must be > 0 (compile-time const, enforced by type system).
const SHARD_COUNT: usize = 16;

/// A single shard of the MemTable, holding a subset of the key space.
#[derive(Debug)]
struct MemTableShard {
    entries: RwLock<MemTableMap>,
}

#[derive(Debug)]
pub struct MemTable {
    /// Sharded storage: each shard holds a disjoint subset of keys.
    /// Shard selection is deterministic via `shard_for()`.
    shards: [MemTableShard; SHARD_COUNT],
    size: AtomicUsize,
    min_tx: AtomicU64,
    max_tx: AtomicU64,
}

impl MemTable {
    /// Creates a new empty MemTable with `SHARD_COUNT` independent shards.
    pub fn new() -> Self {
        Self {
            shards: std::array::from_fn(|_| MemTableShard {
                entries: RwLock::new(BTreeMap::new()),
            }),
            size: AtomicUsize::new(0),
            min_tx: AtomicU64::new(u64::MAX),
            max_tx: AtomicU64::new(0),
        }
    }

    /// Deterministic shard selector based on the first byte of the key.
    ///
    /// All MemFuse keys are namespaced with a fixed-length prefix (minimum
    /// 10 bytes), so the first byte is always present. For an empty key
    /// (which cannot occur in practice), we default to shard 0.
    #[inline]
    fn shard_for(key: &[u8]) -> usize {
        // Zero-panic: modulo a compile-time const > 0.
        key.first().copied().unwrap_or(0) as usize % SHARD_COUNT
    }

    /// Inserts a key-value pair with a sequence number and transaction ID.
    pub fn put(&self, key: Bytes, value: Bytes, seq_no: u64, tx_id: u64) {
        let additional_size = key.len() + value.len() + 16; // Added tx_id
        let shard_idx = Self::shard_for(&key);
        let mut entries = self.shards[shard_idx].entries.write();

        let versions = entries.entry(key).or_default();
        versions.push((seq_no, value, tx_id));

        // Note: Simple size tracking (sums all versions)
        self.size.fetch_add(additional_size, Ordering::Relaxed);

        // Track TX range
        loop {
            let current_min = self.min_tx.load(Ordering::Acquire);
            if tx_id >= current_min
                || self
                    .min_tx
                    .compare_exchange(current_min, tx_id, Ordering::Release, Ordering::Relaxed)
                    .is_ok()
            {
                break;
            }
        }
        loop {
            let current_max = self.max_tx.load(Ordering::Acquire);
            if tx_id <= current_max
                || self
                    .max_tx
                    .compare_exchange(current_max, tx_id, Ordering::Release, Ordering::Relaxed)
                    .is_ok()
            {
                break;
            }
        }
    }

    /// Returns the transaction ID range covered by this MemTable.
    pub fn tx_range(&self) -> (u64, u64) {
        (
            self.min_tx.load(Ordering::Acquire),
            self.max_tx.load(Ordering::Acquire),
        )
    }

    /// Retrieves the latest value and sequence number by key.
    pub fn get(&self, key: &[u8]) -> Option<(Bytes, u64)> {
        let shard_idx = Self::shard_for(key);
        let entries = self.shards[shard_idx].entries.read();
        entries
            .get(key)
            .and_then(|versions| versions.last().map(|(seq, val, _tx)| (val.clone(), *seq)))
    }

    /// Retrieves a value, sequence number, and transaction ID by key at or below a specific sequence number
    /// and bounded by maximum transaction ID for Snapshot Isolation.
    pub fn get_at_seq(&self, key: &[u8], seq_no: u64, max_tx: u64) -> Option<(Bytes, u64, u64)> {
        let shard_idx = Self::shard_for(key);
        let entries = self.shards[shard_idx].entries.read();
        let versions = entries.get(key)?;

        use memfuse_core::{TxId, TOMBSTONE_BIT};

        let idx = match versions.binary_search_by_key(&seq_no, |(s, _, _)| *s & !TOMBSTONE_BIT) {
            Ok(i) => i,
            Err(i) => {
                if i == 0 {
                    return None;
                }
                i - 1
            }
        };

        // Linear search backwards for the latest version satisfying (tx <= max_tx || tx >= INTERNAL_BASE)
        for i in (0..=idx).rev() {
            let (s, v, tx) = &versions[i];
            if *tx <= max_tx || *tx >= TxId::INTERNAL_BASE {
                return Some((v.clone(), *s, *tx));
            }
        }
        None
    }

    /// Returns the approximate size in bytes.
    pub fn size(&self) -> usize {
        self.size.load(Ordering::Relaxed)
    }

    /// Returns true if the memtable is empty.
    ///
    /// WARNING: This reads all shards sequentially. The result is not an
    /// atomic snapshot, but this method is only called on the flush guard
    /// path which is a rare, single-threaded check.
    pub fn is_empty(&self) -> bool {
        self.shards.iter().all(|s| s.entries.read().is_empty())
    }

    /// Iterates over all entries (all versions) in sorted key order.
    /// Returns (Key, Value, SeqNo, TxId).
    ///
    /// Collects from all shards and sorts by key to maintain the global
    /// sorted-order invariant. Called only during flush (low-frequency).
    pub fn iter(&self) -> Vec<(Bytes, Bytes, u64, u64)> {
        let mut results = Vec::new();
        for shard in &self.shards {
            let entries = shard.entries.read();
            for (k, versions) in entries.iter() {
                for (seq, val, tx) in versions {
                    results.push((k.clone(), val.clone(), *seq, *tx));
                }
            }
        }
        // Restore global sorted order: primary by key, secondary by seq_no
        // within versions of the same key (already sorted per-shard via BTreeMap
        // + push order, but cross-shard merge requires a sort).
        results.sort_by(|a, b| a.0.cmp(&b.0).then(a.2.cmp(&b.2)));
        results
    }

    /// Iterates over only the latest version of each key in sorted key order.
    /// Returns (Key, Value, SeqNo, TxId).
    ///
    /// Collects from all shards and sorts by key. Called only during flush.
    pub fn iter_latest(&self) -> Vec<(Bytes, Bytes, u64, u64)> {
        let mut results = Vec::new();
        for shard in &self.shards {
            let entries = shard.entries.read();
            for (k, versions) in entries.iter() {
                if let Some((seq, val, tx)) = versions.last() {
                    results.push((k.clone(), val.clone(), *seq, *tx));
                }
            }
        }
        // Restore global sorted order by key.
        results.sort_by(|a, b| a.0.cmp(&b.0));
        results
    }
}

impl Default for MemTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_put_get() {
        let mt = MemTable::new();
        mt.put(Bytes::from("key1"), Bytes::from("val1"), 1, 1);
        mt.put(Bytes::from("key2"), Bytes::from("val2"), 2, 2);

        let (val, seq) = mt.get(b"key1").expect("key1 should exist"); // expect
        assert_eq!(val.as_ref(), b"val1");
        assert_eq!(seq, 1);

        let (val, seq) = mt.get(b"key2").expect("key2 should exist"); // expect
        assert_eq!(val.as_ref(), b"val2");
        assert_eq!(seq, 2);

        assert!(mt.get(b"key3").is_none());
    }

    #[test]
    fn test_mvcc_get_at_seq() {
        let mt = MemTable::new();
        mt.put(Bytes::from("key1"), Bytes::from("v1"), 10, 1);
        mt.put(Bytes::from("key1"), Bytes::from("v2"), 20, 2);
        mt.put(Bytes::from("key1"), Bytes::from("v3"), 30, 3);

        // Before any version
        assert!(mt.get_at_seq(b"key1", 5, u64::MAX).is_none());

        // Exact match
        let (val, seq, tx) = mt.get_at_seq(b"key1", 20, u64::MAX).unwrap();
        assert_eq!(val.as_ref(), b"v2");
        assert_eq!(seq, 20);
        assert_eq!(tx, 2);

        // Between versions
        let (val, seq, tx) = mt.get_at_seq(b"key1", 25, u64::MAX).unwrap();
        assert_eq!(val.as_ref(), b"v2");
        assert_eq!(seq, 20);
        assert_eq!(tx, 2);

        // Filtered by max_tx: seq 20 has tx=2, max_tx=1 should fallback to seq 10 tx 1
        let (val, seq, tx) = mt.get_at_seq(b"key1", 25, 1).unwrap();
        assert_eq!(val.as_ref(), b"v1");
        assert_eq!(seq, 10);
        assert_eq!(tx, 1);

        // Latest version
        let (val, seq, tx) = mt.get_at_seq(b"key1", 100, u64::MAX).unwrap();
        assert_eq!(val.as_ref(), b"v3");
        assert_eq!(seq, 30);
        assert_eq!(tx, 3);
    }

    #[test]
    fn test_iter_all_versions() {
        let mt = MemTable::new();
        mt.put(Bytes::from("a"), Bytes::from("v1"), 1, 1);
        mt.put(Bytes::from("a"), Bytes::from("v2"), 2, 2);
        mt.put(Bytes::from("b"), Bytes::from("v3"), 3, 3);

        let entries = mt.iter();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].2, 1);
        assert_eq!(entries[1].2, 2);
        assert_eq!(entries[2].2, 3);
    }

    #[test]
    fn test_mvcc_tombstone_binary_search() {
        use memfuse_core::TOMBSTONE_BIT;
        let mt = MemTable::new();
        let key = Bytes::from("key1");

        // Insert a value then a tombstone at a higher sequence number
        mt.put(key.clone(), Bytes::from("val1"), 10, 1);
        mt.put(key.clone(), Bytes::new(), 20 | TOMBSTONE_BIT, 2);

        // Read at seq 15 -> should get val1
        let (val, seq, tx) = mt.get_at_seq(&key, 15, u64::MAX).expect("Should find v1");
        assert_eq!(val.as_ref(), b"val1");
        assert_eq!(seq, 10);
        assert_eq!(tx, 1);

        // Read at seq 25 -> should get tombstone
        let (val, seq, tx) = mt
            .get_at_seq(&key, 25, u64::MAX)
            .expect("Should find tombstone");
        assert_eq!(val.len(), 0);
        assert_eq!(seq, 20 | TOMBSTONE_BIT);
        assert_eq!(tx, 2);
    }

    #[test]
    fn test_iter_latest() {
        let mt = MemTable::new();
        mt.put(Bytes::from("a"), Bytes::from("v1"), 1, 1);
        mt.put(Bytes::from("a"), Bytes::from("v2"), 2, 2);
        mt.put(Bytes::from("b"), Bytes::from("v3"), 3, 3);

        let entries = mt.iter_latest();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].0.as_ref(), b"a");
        assert_eq!(entries[0].2, 2);
        assert_eq!(entries[1].0.as_ref(), b"b");
        assert_eq!(entries[1].2, 3);
    }

    #[test]
    fn test_concurrent_put_no_data_loss() {
        // Write 1000 keys from 8 threads, verify all are readable.
        use std::sync::Arc;
        let mt = Arc::new(MemTable::new());
        let handles: Vec<_> = (0..8u64)
            .map(|t| {
                let mt = Arc::clone(&mt);
                std::thread::spawn(move || {
                    for i in 0..125u64 {
                        let key = Bytes::from(format!("key-{}-{}", t, i));
                        mt.put(key, Bytes::from("val"), t * 125 + i + 1, t);
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().expect("thread panicked"); // #[cfg(test)]
        }
        assert_eq!(mt.iter_latest().len(), 1000);
    }
}
