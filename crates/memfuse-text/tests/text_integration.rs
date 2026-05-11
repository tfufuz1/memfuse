use memfuse_core::{DocId, TxId, StorageEngine};
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use memfuse_text::inverted::InvertedIndex;
use tempfile::TempDir;
use std::sync::Arc;

// ANCHOR:INTEGRATION:TEXT-001 — Text Pipeline Integration Test
// AGENT:12 DATE:2026-05-09 STATUS:DONE
#[tokio::test]
async fn test_text_pipeline_full() {
    let tmp = TempDir::new().unwrap();
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        ..Default::default()
    };
    let storage = Arc::new(LsmStorage::new(config).await.unwrap());
    let index = InvertedIndex::new(storage.clone(), "default");

    // 1. Ingest
    let tx1 = TxId::new(1);
    index.upsert_document(tx1, DocId::new(1), "The quick brown fox").await.unwrap();
    index.upsert_document(tx1, DocId::new(2), "Jumps over the lazy dog").await.unwrap();
    storage.commit(tx1).await.unwrap();

    // 2. Search
    let results = index.search_bm25("fox", 10).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, DocId::new(1));

    // 3. Update
    let tx2 = TxId::new(2);
    index.upsert_document(tx2, DocId::new(1), "The quick brown fox is fast").await.unwrap();
    storage.commit(tx2).await.unwrap();

    let results = index.search_bm25("fast", 10).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, DocId::new(1));

    // 4. Delete
    let tx3 = TxId::new(3);
    index.delete_document(tx3, DocId::new(1), "The quick brown fox is fast").await.unwrap();
    storage.commit(tx3).await.unwrap();

    let results = index.search_bm25("fox", 10).await.unwrap();
    assert_eq!(results.len(), 0);
}
