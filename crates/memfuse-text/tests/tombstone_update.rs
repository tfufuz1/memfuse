//! SD-05-TEXT-001 — Tombstone update path integration tests.
//!
//! Verifies that `upsert_document` no longer eagerly deletes old posting-list
//! entries on updates (tombstone path) while maintaining full BM25 correctness.

use memfuse_core::{DocId, Result, StorageEngine, TxId};
use memfuse_text::InvertedIndex;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Shared MockStorage (mirrors the one in inverted.rs tests)
// ---------------------------------------------------------------------------

struct MockStorage {
    store: RwLock<HashMap<Vec<u8>, Vec<u8>>>,
    delete_calls: std::sync::atomic::AtomicU64,
}

impl MockStorage {
    fn new() -> Self {
        Self {
            store: RwLock::new(HashMap::new()),
            delete_calls: std::sync::atomic::AtomicU64::new(0),
        }
    }

    fn delete_call_count(&self) -> u64 {
        self.delete_calls.load(std::sync::atomic::Ordering::Relaxed)
    }
}

#[async_trait::async_trait]
impl StorageEngine for MockStorage {
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        Ok(self.store.read().get(key).cloned())
    }
    async fn put(&self, _tx: TxId, key: &[u8], value: &[u8]) -> Result<()> {
        self.store.write().insert(key.to_vec(), value.to_vec());
        Ok(())
    }
    async fn delete(&self, _tx: TxId, key: &[u8]) -> Result<()> {
        self.delete_calls
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.store.write().remove(key);
        Ok(())
    }
    async fn commit(&self, _tx: TxId) -> Result<()> {
        Ok(())
    }
    async fn rollback(&self, _tx: TxId) -> Result<()> {
        Ok(())
    }
    async fn rollback_to_tx(&self, _tx: TxId) -> Result<()> {
        Ok(())
    }
    async fn get_at_seq(&self, key: &[u8], _seq: u64) -> Result<Option<Vec<u8>>> {

        self.get(key).await
    }
    async fn last_seq_no(&self) -> Result<u64> {
        Ok(0)
    }
    async fn last_tx_id(&self) -> Result<TxId> {
        Ok(TxId::new(0))
    }
    async fn flush(&self) -> Result<()> {
        Ok(())
    }
    async fn stats(&self) -> Result<memfuse_core::StorageStats> {
        Ok(memfuse_core::StorageStats {
            num_segments: 0,
            total_size_bytes: 0,
            memtable_size_bytes: 0,
        })
    }
    async fn pin_checkpoint(&self, _id: u64) -> Result<()> {
        Ok(())
    }
    async fn unpin_checkpoint(&self, _id: u64) -> Result<()> {
        Ok(())
    }
    async fn scan(
        &self,
        _start: std::ops::Bound<&[u8]>,
        _end: std::ops::Bound<&[u8]>,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        Ok(Vec::new())
    }
    async fn scan_prefix(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let store = self.store.read();
        Ok(store
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// On update the tombstone path must NOT call `storage.delete` for old
/// posting-list entries — those are overwritten lazily by LSM semantics.
/// Only `resolve_tombstones()` should trigger deletions.
#[tokio::test]
async fn test_tombstone_update_no_eager_delete(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let storage = Arc::new(MockStorage::new());
    let index = InvertedIndex::new(storage.clone(), "default");

    let d1 = DocId::new(1);

    // First insert — no prior state, zero deletes expected.
    let tx1 = TxId::new(1);
    index.upsert_document(tx1, d1, "rust programming").await?;
    storage.commit(tx1).await?;
    let deletes_after_insert = storage.delete_call_count();
    assert_eq!(deletes_after_insert, 0, "First insert must not call delete");

    // Update — tombstone path: still zero eager deletes.
    let tx2 = TxId::new(2);
    index.upsert_document(tx2, d1, "python coding").await?;
    storage.commit(tx2).await?;
    let deletes_after_update = storage.delete_call_count();
    assert_eq!(
        deletes_after_update, 0,
        "Update must not eagerly delete old posting-list entries (tombstone path)"
    );

    // A tombstone key tbs:1 should now exist.
    let tbs_prefix = b"__txt:default:tbs:";
    let tbs_entries = storage.scan_prefix(tbs_prefix).await?;
    assert_eq!(tbs_entries.len(), 1, "Exactly one tombstone expected");

    Ok(())
}

/// After `resolve_tombstones()` the stale entries from the old document version
/// should be removed and the BM25 index should reflect only the new terms.
#[tokio::test]
async fn test_bm25_correct_after_tombstone_update(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let storage = Arc::new(MockStorage::new());
    let index = InvertedIndex::new(storage.clone(), "resolve");

    let d1 = DocId::new(1);
    let d2 = DocId::new(2);

    // Insert two docs
    let tx1 = TxId::new(1);
    index.upsert_document(tx1, d1, "rust programming").await?;
    index.upsert_document(tx1, d2, "python coding").await?;
    storage.commit(tx1).await?;

    // Update d1: "rust programming" → "python coding"
    let tx2 = TxId::new(2);
    index.upsert_document(tx2, d1, "python coding").await?;
    storage.commit(tx2).await?;

    // Before resolve_tombstones: "rust" may still appear for d1 (stale entry)
    // After resolve_tombstones: "rust" must NOT appear for d1.

    let tx3 = TxId::new(3);
    let resolved = index.resolve_tombstones(tx3).await?;
    storage.commit(tx3).await?;
    assert_eq!(resolved, 1, "One tombstone should be resolved");

    // "rust" should now have zero results (stale d1 entry removed)
    let rust_results = index.search_bm25("rust", 10).await?;
    assert_eq!(
        rust_results.len(),
        0,
        "After resolve_tombstones 'rust' entry for d1 must be removed"
    );

    // "python" should return both d1 and d2
    let python_results = index.search_bm25("python", 10).await?;
    assert_eq!(python_results.len(), 2);

    // "coding" should return both d1 and d2
    let coding_results = index.search_bm25("coding", 10).await?;
    assert_eq!(coding_results.len(), 2);

    Ok(())
}

/// Multiple sequential updates of the same document should accumulate only
/// ONE tombstone (because the same key is overwritten each time).
#[tokio::test]
async fn test_multiple_updates_single_tombstone(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let storage = Arc::new(MockStorage::new());
    let index = InvertedIndex::new(storage.clone(), "multi");

    let d1 = DocId::new(1);
    let tbs_prefix = b"__txt:multi:tbs:";

    let tx1 = TxId::new(1);
    index.upsert_document(tx1, d1, "first version").await?;
    storage.commit(tx1).await?;

    for (i, text) in ["second version", "third version", "fourth version"]
        .iter()
        .enumerate()
    {
        let tx = TxId::new(2 + i as u64);
        index.upsert_document(tx, d1, text).await?;
        storage.commit(tx).await?;
    }

    // Should still be exactly one tombstone for d1 — same key overwritten.
    let tbs_entries = storage.scan_prefix(tbs_prefix).await?;
    assert_eq!(
        tbs_entries.len(),
        1,
        "Multiple updates should yield exactly one tombstone per document"
    );

    Ok(())
}
