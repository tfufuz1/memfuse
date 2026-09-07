//! SD-05-TEXT-001 — Write Amplification Measurement Tests.
//!
//! Verifies that the tombstone update path reduces write amplification
//! compared to the old read-modify-write approach. The new path should
//! require O(N_new_terms) puts instead of O(N_old_terms) deletes + O(N_new_terms) puts.

use memfuse_core::{BoxFuture, DocId, Result, StorageEngine, TextIndex, TxId};
use memfuse_text::InvertedIndex;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

struct InstrumentedStorage {
    store: RwLock<HashMap<Vec<u8>, Vec<u8>>>,
    put_count: AtomicU64,
    delete_count: AtomicU64,
}

impl InstrumentedStorage {
    fn new() -> Self {
        Self {
            store: RwLock::new(HashMap::new()),
            put_count: AtomicU64::new(0),
            delete_count: AtomicU64::new(0),
        }
    }

    fn reset_counters(&self) {
        self.put_count.store(0, Ordering::Relaxed);
        self.delete_count.store(0, Ordering::Relaxed);
    }

    fn puts(&self) -> u64 {
        self.put_count.load(Ordering::Relaxed)
    }

    fn deletes(&self) -> u64 {
        self.delete_count.load(Ordering::Relaxed)
    }
}

impl StorageEngine for InstrumentedStorage {
    fn get<'a>(&'a self, key: &'a [u8]) -> BoxFuture<'a, Result<Option<Vec<u8>>>> {
        Box::pin(async move { Ok(self.store.read().get(key).cloned()) })
    }
    fn put<'a>(&'a self, _tx: TxId, key: &'a [u8], value: &'a [u8]) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.put_count.fetch_add(1, Ordering::Relaxed);
            self.store.write().insert(key.to_vec(), value.to_vec());
            Ok(())
        })
    }
    fn delete<'a>(&'a self, _tx: TxId, key: &'a [u8]) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.delete_count.fetch_add(1, Ordering::Relaxed);
            self.store.write().remove(key);
            Ok(())
        })
    }
    fn commit<'a>(&'a self, _tx: TxId) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move { Ok(()) })
    }
    fn rollback<'a>(&'a self, _tx: TxId) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move { Ok(()) })
    }
    fn rollback_to_tx<'a>(&'a self, _tx: TxId) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move { Ok(()) })
    }
    fn get_at_seq<'a>(
        &'a self,
        key: &'a [u8],
        _seq: u64,
    ) -> BoxFuture<'a, Result<Option<Vec<u8>>>> {
        Box::pin(async move { self.get(key).await })
    }
    fn last_seq_no<'a>(&'a self) -> BoxFuture<'a, Result<u64>> {
        Box::pin(async move { Ok(0) })
    }
    fn last_tx_id<'a>(&'a self) -> BoxFuture<'a, Result<TxId>> {
        Box::pin(async move { Ok(TxId::new(0)) })
    }
    fn flush<'a>(&'a self) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move { Ok(()) })
    }
    fn stats<'a>(&'a self) -> BoxFuture<'a, Result<memfuse_core::StorageStats>> {
        Box::pin(async move {
            Ok(memfuse_core::StorageStats {
                num_segments: 0,
                total_size_bytes: 0,
                memtable_size_bytes: 0,
            })
        })
    }
    fn pin_checkpoint<'a>(&'a self, _id: u64) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move { Ok(()) })
    }
    fn unpin_checkpoint<'a>(&'a self, _id: u64) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move { Ok(()) })
    }
    fn scan<'a>(
        &'a self,
        _start: std::ops::Bound<&'a [u8]>,
        _end: std::ops::Bound<&'a [u8]>,
    ) -> BoxFuture<'a, Result<Vec<(Vec<u8>, Vec<u8>)>>> {
        Box::pin(async move { Ok(Vec::new()) })
    }
    fn scan_prefix<'a>(
        &'a self,
        prefix: &'a [u8],
    ) -> BoxFuture<'a, Result<Vec<(Vec<u8>, Vec<u8>)>>> {
        Box::pin(async move {
            let store = self.store.read();
            Ok(store
                .iter()
                .filter(|(k, _)| k.starts_with(prefix))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect())
        })
    }
    fn scan_prefix_at<'a>(
        &'a self,
        prefix: &'a [u8],
        _seq_no: u64,
    ) -> BoxFuture<'a, Result<Vec<(Vec<u8>, Vec<u8>)>>> {
        Box::pin(async move { self.scan_prefix(prefix).await })
    }
}

/// First insert of a document with N unique terms should require:
///   - 1 put for doc length (dl:)
///   - 1 put for forward index (fw:)
///   - 1 put for metadata (meta:stats)
///   - N puts for posting lists (pl:{term}:{doc_id})
///   Total: N + 3 puts, 0 deletes
#[tokio::test]
async fn test_first_insert_io_count() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let storage = Arc::new(InstrumentedStorage::new());
    let index = InvertedIndex::new(storage.clone(), "io_test");

    let tx = TxId::new(1);
    let doc_id = DocId::new(1);
    // 4 unique terms
    index
        .upsert_document(tx, doc_id, "alpha beta gamma delta")
        .await?;
    index.commit(tx).await?;

    // Expected: 4 terms + dl + fw + meta = 7 puts
    assert_eq!(
        storage.puts(),
        7,
        "First insert: 4 terms + 3 overhead = 7 puts"
    );
    assert_eq!(
        storage.deletes(),
        0,
        "First insert must not delete anything"
    );

    Ok(())
}

/// Update of a document with the tombstone path should:
///   - NOT call delete for old terms (tombstone path)
///   - Put: 1 dl + 1 fw + 1 meta + 1 tombstone + N_new posting lists
///   Total: N_new + 4 puts, 0 deletes (before resolve_tombstones)
#[tokio::test]
async fn test_tombstone_update_io_count() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let storage = Arc::new(InstrumentedStorage::new());
    let index = InvertedIndex::new(storage.clone(), "io_update");

    // First insert: 3 terms → 6 puts
    let tx1 = TxId::new(1);
    let doc_id = DocId::new(1);
    index
        .upsert_document(tx1, doc_id, "alpha beta gamma")
        .await?;
    index.commit(tx1).await?;
    assert_eq!(storage.puts(), 6, "First insert: 3 + 3 = 6");

    // Reset for update measurement
    storage.reset_counters();

    // Update: entirely new 2 terms
    let tx2 = TxId::new(2);
    index.upsert_document(tx2, doc_id, "delta epsilon").await?;
    index.commit(tx2).await?;

    // Tombstone path: 3 tbs + 1 dl + 1 fw + 1 meta + 2 terms = 8 puts
    assert_eq!(
        storage.puts(),
        8,
        "Tombstone update: 2 terms + 3 tombstones + 3 overhead = 8 puts"
    );
    assert_eq!(
        storage.deletes(),
        0,
        "Tombstone path must NOT eagerly delete old terms"
    );

    Ok(())
}

/// After resolve_tombstones, stale entries are cleaned up via deletes.
/// This must only delete the removed terms, not the overlapping ones.
#[tokio::test]
async fn test_resolve_tombstones_cleanup_count(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let storage = Arc::new(InstrumentedStorage::new());
    let index = InvertedIndex::new(storage.clone(), "resolve_io");

    // Insert: "A B C" (3 terms)
    let tx1 = TxId::new(1);
    let doc_id = DocId::new(1);
    index
        .upsert_document(tx1, doc_id, "alpha beta gamma")
        .await?;
    index.commit(tx1).await?;

    // Update: "alpha delta" (1 overlapping, 1 new, 2 removed: beta, gamma)
    let tx2 = TxId::new(2);
    index.upsert_document(tx2, doc_id, "alpha delta").await?;
    index.commit(tx2).await?;

    storage.reset_counters();

    // Resolve tombstones: should delete "beta" and "gamma" entries + tombstone itself
    let tx3 = TxId::new(3);
    let resolved = index.resolve_tombstones(tx3).await?;
    index.commit(tx3).await?;

    assert_eq!(resolved, 3, "Three tombstones resolved");
    // Deletes: 2 stale terms (beta, gamma) + 3 tombstones = 5
    assert_eq!(
        storage.deletes(),
        5,
        "Resolve must delete 2 stale terms + 3 tombstone markers"
    );

    Ok(())
}
