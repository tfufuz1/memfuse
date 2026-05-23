//! In-memory sorted MemTable for the LSM-Tree with MVCC support.
//!
//! Entries are stored in a `BTreeMap` where each key maps to a versioned list
//! of values. This enables Snapshot Isolation by allowing point-in-time reads.

// ANCHOR:ARCH:MEMTABLE-001 — In-Memory Sortierter Puffer (hot writes).
// WP:WP-0.0 PRIO:1 NEEDS:NONE
// AGENT:01 DATE:2026-05-23 STATUS:DONE (MVCC Refactor)
// CREATED:2026-05-05 DEADLINE:NONE

use bytes::Bytes;
use parking_lot::RwLock;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

/// An in-memory sorted key-value structure with MVCC support.
#[derive(Debug)]
pub struct MemTable {
    /// Maps UserKey -> Vec<(SequenceNumber, Value)>.
    /// The Vec is sorted by SequenceNumber ascending.
    entries: RwLock<BTreeMap<Bytes, Vec<(u64, Bytes)>>>,
    size: AtomicUsize,
}

impl MemTable {
    /// Creates a new empty MemTable.
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(BTreeMap::new()),
            size: AtomicUsize::new(0),
        }
    }

    /// Inserts a key-value pair with a sequence number.
    pub fn put(&self, key: Bytes, value: Bytes, seq_no: u64) {
        let additional_size = key.len() + value.len() + 8;
        let mut entries = self.entries.write();

        let versions = entries.entry(key).or_default();
        versions.push((seq_no, value));

        // Note: Simple size tracking (sums all versions)
        self.size.fetch_add(additional_size, Ordering::Relaxed);
    }

    /// Retrieves the latest value and sequence number by key.
    pub fn get(&self, key: &[u8]) -> Option<(Bytes, u64)> {
        let entries = self.entries.read();
        entries
            .get(key)
            .and_then(|versions| versions.last().map(|(seq, val)| (val.clone(), *seq)))
    }

    /// Retrieves a value and sequence number by key at or below a specific sequence number.
    pub fn get_at_seq(&self, key: &[u8], seq_no: u64) -> Option<(Bytes, u64)> {
        let entries = self.entries.read();
        let versions = entries.get(key)?;

        use memfuse_core::TOMBSTONE_BIT;

        // Binary search for the latest version <= seq_no
        // We must mask the TOMBSTONE_BIT during comparison because the search key
        // is a clean sequence number.
        match versions.binary_search_by_key(&seq_no, |(s, _)| *s & !TOMBSTONE_BIT) {
            Ok(idx) => {
                let (s, v) = &versions[idx];
                Some((v.clone(), *s))
            }
            Err(idx) => {
                if idx == 0 {
                    None
                } else {
                    let (s, v) = &versions[idx - 1];
                    Some((v.clone(), *s))
                }
            }
        }
    }

    /// Returns the approximate size in bytes.
    pub fn size(&self) -> usize {
        self.size.load(Ordering::Relaxed)
    }

    /// Returns true if the memtable is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.read().is_empty()
    }

    /// Iterates over all entries (all versions) in sorted order.
    /// Returns (Key, Value, SeqNo).
    pub fn iter(&self) -> Vec<(Bytes, Bytes, u64)> {
        let entries = self.entries.read();
        let mut results = Vec::new();
        for (k, versions) in entries.iter() {
            for (seq, val) in versions {
                results.push((k.clone(), val.clone(), *seq));
            }
        }
        results
    }

    /// Iterates over only the latest version of each key in sorted order.
    /// Returns (Key, Value, SeqNo).
    pub fn iter_latest(&self) -> Vec<(Bytes, Bytes, u64)> {
        let entries = self.entries.read();
        let mut results = Vec::with_capacity(entries.len());
        for (k, versions) in entries.iter() {
            if let Some((seq, val)) = versions.last() {
                results.push((k.clone(), val.clone(), *seq));
            }
        }
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
        mt.put(Bytes::from("key1"), Bytes::from("val1"), 1);
        mt.put(Bytes::from("key2"), Bytes::from("val2"), 2);

        let (val, seq) = mt.get(b"key1").expect("key1 should exist");
        assert_eq!(val.as_ref(), b"val1");
        assert_eq!(seq, 1);

        let (val, seq) = mt.get(b"key2").expect("key2 should exist");
        assert_eq!(val.as_ref(), b"val2");
        assert_eq!(seq, 2);

        assert!(mt.get(b"key3").is_none());
    }

    #[test]
    fn test_mvcc_get_at_seq() {
        let mt = MemTable::new();
        mt.put(Bytes::from("key1"), Bytes::from("v1"), 10);
        mt.put(Bytes::from("key1"), Bytes::from("v2"), 20);
        mt.put(Bytes::from("key1"), Bytes::from("v3"), 30);

        // Before any version
        assert!(mt.get_at_seq(b"key1", 5).is_none());

        // Exact match
        let (val, seq) = mt.get_at_seq(b"key1", 20).unwrap() // unwrap;
        assert_eq!(val.as_ref(), b"v2");
        assert_eq!(seq, 20);

        // Between versions
        let (val, seq) = mt.get_at_seq(b"key1", 25).unwrap() // unwrap;
        assert_eq!(val.as_ref(), b"v2");
        assert_eq!(seq, 20);

        // Latest version
        let (val, seq) = mt.get_at_seq(b"key1", 100).unwrap() // unwrap;
        assert_eq!(val.as_ref(), b"v3");
        assert_eq!(seq, 30);
    }

    #[test]
    fn test_iter_all_versions() {
        let mt = MemTable::new();
        mt.put(Bytes::from("a"), Bytes::from("v1"), 1);
        mt.put(Bytes::from("a"), Bytes::from("v2"), 2);
        mt.put(Bytes::from("b"), Bytes::from("v3"), 3);

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
        mt.put(key.clone(), Bytes::from("val1"), 10);
        mt.put(key.clone(), Bytes::new(), 20 | TOMBSTONE_BIT);

        // Read at seq 15 -> should get val1
        let (val, seq) = mt.get_at_seq(&key, 15).expect("Should find v1");
        assert_eq!(val.as_ref(), b"val1");
        assert_eq!(seq, 10);

        // Read at seq 25 -> should get tombstone
        let (val, seq) = mt.get_at_seq(&key, 25).expect("Should find tombstone");
        assert_eq!(val.len(), 0);
        assert_eq!(seq, 20 | TOMBSTONE_BIT);
    }

    #[test]
    fn test_iter_latest() {
        let mt = MemTable::new();
        mt.put(Bytes::from("a"), Bytes::from("v1"), 1);
        mt.put(Bytes::from("a"), Bytes::from("v2"), 2);
        mt.put(Bytes::from("b"), Bytes::from("v3"), 3);

        let entries = mt.iter_latest();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].0.as_ref(), b"a");
        assert_eq!(entries[0].2, 2);
        assert_eq!(entries[1].0.as_ref(), b"b");
        assert_eq!(entries[1].2, 3);
    }
}
