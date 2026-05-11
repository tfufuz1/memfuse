use memfuse_core::{StorageEngine, TxId};
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use std::sync::Arc;
use tempfile::TempDir;

// ANCHOR:INTEGRATION:STORE-001 — LSM Integration Test
// AGENT:12 DATE:2026-05-09 STATUS:DONE
#[tokio::test]
async fn test_lsm_full_pipeline() {
    let tmp = TempDir::new().unwrap();
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        ..Default::default()
    };
    let storage = Arc::new(LsmStorage::new(config).await.unwrap());

    // 1. Put and Commit
    let tx1 = TxId::new(1);
    storage.put(tx1, b"key1", b"val1").await.unwrap();
    storage.commit(tx1).await.unwrap();

    // 2. Get
    let val = storage
        .get(b"key1")
        .await
        .unwrap()
        .expect("Value should exist");
    assert_eq!(val, b"val1");

    // 3. Rollback
    let tx2 = TxId::new(2);
    storage.put(tx2, b"key1", b"val2").await.unwrap();
    storage.rollback(tx2).await.unwrap();

    let val = storage
        .get(b"key1")
        .await
        .unwrap()
        .expect("Value should still be val1");
    assert_eq!(val, b"val1");

    // 4. Flush
    storage.flush().await.unwrap();
    let val = storage
        .get(b"key1")
        .await
        .unwrap()
        .expect("Value should exist after flush");
    assert_eq!(val, b"val1");

    // 5. Delete
    let tx3 = TxId::new(3);
    storage.delete(tx3, b"key1").await.unwrap();
    storage.commit(tx3).await.unwrap();

    let val = storage.get(b"key1").await.unwrap();
    assert!(val.is_none());
}
