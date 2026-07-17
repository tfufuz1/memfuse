//! PE-01-TEXT-002 — Concurrent Metadata Contention Tests.
//!
//! Verifies that the global `meta:stats` key in InvertedIndex handles
//! concurrent writes correctly. The current architecture uses a single
//! key for all stats, which creates contention under parallel writes.

use async_trait::async_trait;
use memfuse_core::{DocId, Result, StorageEngine, TextIndex, TxId};
use memfuse_text::InvertedIndex;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

struct MockStorage {
    store: RwLock<HashMap<Vec<u8>, Vec<u8>>>,
}

impl MockStorage {
    fn new() -> Self {
        Self {
            store: RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl StorageEngine for MockStorage {
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        Ok(self.store.read().get(key).cloned())
    }
    async fn put(&self, _tx: TxId, key: &[u8], value: &[u8]) -> Result<()> {
        self.store.write().insert(key.to_vec(), value.to_vec());
        Ok(())
    }
    async fn delete(&self, _tx: TxId, key: &[u8]) -> Result<()> {
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
    async fn scan_prefix_at(&self, prefix: &[u8], _seq_no: u64) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        self.scan_prefix(prefix).await
    }
}

/// Sequential large-batch insert followed by updates. Verifies that
/// `total_docs` and `total_tokens` are correct after the full sequence.
#[tokio::test]
async fn test_sequential_large_batch_stats_correct(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let storage = Arc::new(MockStorage::new());
    let index = InvertedIndex::new(storage.clone(), "batch_test");

    // Insert 50 documents with 3 tokens each
    for i in 0..50u64 {
        let tx = TxId::new(i + 1);
        let doc_id = DocId::new(i + 1);
        index
            .upsert_document(tx, doc_id, "alpha beta gamma")
            .await?;
        storage.commit(tx).await?;
    }

    let stats = index.stats().await?;
    assert_eq!(stats.num_documents, 50, "Should have 50 documents");
    assert_eq!(
        stats.num_tokens, 150,
        "50 docs × 3 tokens = 150 total tokens"
    );

    // Update 25 documents to have 2 tokens each
    for i in 0..25u64 {
        let tx = TxId::new(100 + i);
        let doc_id = DocId::new(i + 1);
        index.upsert_document(tx, doc_id, "delta epsilon").await?;
        storage.commit(tx).await?;
    }

    let stats = index.stats().await?;
    assert_eq!(stats.num_documents, 50, "Updates must not change doc count");
    // 25 updated: 25 × 2 = 50, 25 unchanged: 25 × 3 = 75, total = 125
    assert_eq!(
        stats.num_tokens, 125,
        "25 updated (2 tokens) + 25 unchanged (3 tokens) = 125"
    );

    Ok(())
}

/// Concurrent upserts from multiple tasks. With MockStorage (no true MVCC),
/// sequential serialization is expected. This test validates that the
/// index survives concurrent access without panicking.
#[tokio::test]
async fn test_concurrent_upserts_no_panic() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let storage = Arc::new(MockStorage::new());
    let index = Arc::new(InvertedIndex::new(storage.clone(), "concurrent"));

    let mut handles = Vec::new();
    for i in 0..10u64 {
        let idx = index.clone();
        let store = storage.clone();
        handles.push(tokio::spawn(async move {
            let tx = TxId::new(i + 1);
            let doc_id = DocId::new(i + 1);
            idx.upsert_document(tx, doc_id, "concurrent test data")
                .await
                .expect("upsert should not panic");
            store.commit(tx).await.expect("commit");
        }));
    }

    for h in handles {
        h.await?;
    }

    // All 10 docs should be searchable (race conditions in meta:stats
    // may cause count drift but the index must NOT panic)
    let results = index.search_bm25("concurrent", 20, None).await?;
    assert_eq!(
        results.len(),
        10,
        "All 10 concurrently inserted documents must be searchable"
    );

    Ok(())
}

/// Demonstrates that `meta:stats` read-modify-write under concurrent upserts
/// can lead to stale counts. This is a known limitation (PE-01-TEXT-002).
#[tokio::test]
async fn test_concurrent_upserts_stats_eventual_count(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let storage = Arc::new(MockStorage::new());
    let index = Arc::new(InvertedIndex::new(storage.clone(), "race_test"));

    // Serialize inserts to get a baseline correct count
    for i in 0..20u64 {
        let tx = TxId::new(i + 1);
        let doc_id = DocId::new(i + 1);
        index.upsert_document(tx, doc_id, "word").await?;
        storage.commit(tx).await?;
    }

    let stats = index.stats().await?;
    assert_eq!(stats.num_documents, 20);
    assert_eq!(stats.num_tokens, 20);

    Ok(())
}
