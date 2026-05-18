// ANCHOR:ARCH:MEMTABLE-001 — In-Memory Sortierter Puffer (hot writes).
// WP:WP-0.0 PRIO:1 NEEDS:NONE
// AGENT:01 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-05 DEADLINE:NONE
// DESIGN: BTreeMap<Bytes, (Bytes, u64)> — Key sortiert für geordneten SSTable-Flush.
// SIZE-TRACKING: AtomicUsize zählt Bytes, LsmStorage flusht bei > memtable_size_limit.
// LIFECYCLE: Active MemTable → Immutable MemTable → Flushed to SSTable → Dropped.
// BENANNT als "SkipList" im Doc-Comment, aber BTreeMap-backed (historischer Name).
//! In-memory SkipList-based MemTable.

use bytes::Bytes;
use parking_lot::RwLock;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

/// An in-memory sorted key-value structure backed by a BTreeMap.
///
/// Entries are sorted by key for efficient range scans and
/// ordered flushing to SSTables.
#[derive(Debug)]
pub struct MemTable {
    entries: RwLock<BTreeMap<Bytes, (Bytes, u64)>>,
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

        if let Some((old_val, _)) = entries.get(&key) {
            // Subtract old entry size
            let old_size = key.len() + old_val.len() + 8;
            self.size.fetch_sub(old_size, Ordering::Relaxed);
        }

        entries.insert(key, (value, seq_no));
        self.size.fetch_add(additional_size, Ordering::Relaxed);
    }

    /// Retrieves a value and sequence number by key.
    pub fn get(&self, key: &[u8]) -> Option<(Bytes, u64)> {
        let entries = self.entries.read();
        entries.get(key).cloned()
    }

    /// Returns the approximate size in bytes.
    pub fn size(&self) -> usize {
        self.size.load(Ordering::Relaxed)
    }

    /// Returns true if the memtable is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.read().is_empty()
    }

    /// Iterates over all entries in sorted order.
    pub fn iter(&self) -> Vec<(Bytes, Bytes, u64)> {
        let entries = self.entries.read();
        entries
            .iter()
            .map(|(k, (v, s))| (k.clone(), v.clone(), *s))
            .collect()
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
    fn test_overwrite() {
        let mt = MemTable::new();
        mt.put(Bytes::from("key1"), Bytes::from("val1"), 1);
        mt.put(Bytes::from("key1"), Bytes::from("val2"), 2);

        let (val, seq) = mt.get(b"key1").expect("key1 should exist");
        assert_eq!(val.as_ref(), b"val2");
        assert_eq!(seq, 2);
    }

    #[test]
    fn test_iter_sorted() {
        let mt = MemTable::new();
        mt.put(Bytes::from("c"), Bytes::from("3"), 3);
        mt.put(Bytes::from("a"), Bytes::from("1"), 1);
        mt.put(Bytes::from("b"), Bytes::from("2"), 2);

        let entries = mt.iter();
        let keys: Vec<_> = entries.iter().map(|(k, _, _)| k.as_ref()).collect();
        assert_eq!(keys, vec![b"a".as_slice(), b"b", b"c"]);
    }
}
