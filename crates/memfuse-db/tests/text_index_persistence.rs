// AGENT:11
use memfuse_core::{DocId, StorageEngine, TextIndex, TxId};
use memfuse_store::{LsmConfig, LsmStorage};
use memfuse_text::inverted::InvertedIndex;
use std::sync::Arc;
use tempfile::TempDir;

// ANCHOR:INTEGRATION:TEXT-001 STATUS:DONE AGENT:12 DATE:2026-06-20
// Test that InvertedIndex correctly persists and retrieves data using LsmStorage.
#[tokio::test]
async fn test_inverted_index_persistence() {
    let tmp = TempDir::new().expect("failed to create temp dir");
    let lsm_config = LsmConfig {
        path: tmp.path().to_path_buf(),
        ..Default::default()
    };
    let storage = Arc::new(
        LsmStorage::new(lsm_config)
            .await
            .expect("failed to open storage"),
    );
    let index = InvertedIndex::new(storage.clone(), "integration-test");

    // 1. Insert documents
    let tx1 = TxId::new(1);
    index
        .insert(tx1, DocId::new(1), "apple")
        .await
        .expect("insert 1 failed");
    storage.commit(tx1).await.expect("commit 1 failed");

    let tx2 = TxId::new(2);
    index
        .insert(tx2, DocId::new(2), "banana")
        .await
        .expect("insert 2 failed");
    storage.commit(tx2).await.expect("commit 2 failed");

    // 2. Verify search
    let results = index
        .search("apple", 10)
        .await
        .expect("search apple failed");
    assert_eq!(
        results.len(),
        1,
        "Expected 1 result for 'apple', got {:?}",
        results
    );
    assert_eq!(results[0].doc_id, DocId::new(1));

    let results_banana = index
        .search("banana", 10)
        .await
        .expect("search banana failed");
    assert_eq!(
        results_banana.len(),
        1,
        "Expected 1 results for 'banana', got {:?}",
        results_banana
    );

    // 3. Drop storage and index (simulating restart)
    drop(index);
    drop(storage);

    // 4. Re-open and verify persistence
    let lsm_config2 = LsmConfig {
        path: tmp.path().to_path_buf(),
        ..Default::default()
    };
    let storage2 = Arc::new(
        LsmStorage::new(lsm_config2)
            .await
            .expect("failed to re-open storage"),
    );
    let index2 = InvertedIndex::new(storage2.clone(), "integration-test");

    let results2 = index2
        .search("apple", 10)
        .await
        .expect("search after restart failed");
    assert_eq!(
        results2.len(),
        1,
        "Expected 1 result for 'apple' after restart, got {:?}",
        results2
    );

    // 5. Delete document and verify
    let tx3 = TxId::new(3);
    index2
        .delete(tx3, DocId::new(1))
        .await
        .expect("delete failed");
    storage2.commit(tx3).await.expect("commit 3 failed");

    let results3 = index2
        .search("apple", 10)
        .await
        .expect("search after delete failed");
    assert_eq!(
        results3.len(),
        0,
        "Expected 0 result for 'apple' after delete, got {:?}",
        results3
    );
}
