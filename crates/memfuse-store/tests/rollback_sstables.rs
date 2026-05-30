use memfuse_core::{StorageEngine, TxId};
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use tempfile::tempdir;

#[tokio::test]
async fn test_rollback_after_flush() {
    let dir = tempdir().unwrap();
    let config = LsmConfig {
        path: dir.path().to_path_buf(),
        memtable_size_limit: 1024, // Small limit to trigger flush easily
        ..Default::default()
    };

    let storage = LsmStorage::new(config)
        .await
        .expect("Failed to create storage");

    // 1. Insert data in TX 1
    let tx1 = TxId::new(1);
    storage.put(tx1, b"key1", b"val1").await.unwrap();
    storage.commit(tx1).await.unwrap();

    // 2. Insert data in TX 2
    let tx2 = TxId::new(2);
    storage.put(tx2, b"key2", b"val2").await.unwrap();
    storage.commit(tx2).await.unwrap();

    // 3. Force flush to create an SSTable with TX 1 and 2
    storage.force_flush().await.unwrap();

    // 4. Insert data in TX 3
    let tx3 = TxId::new(3);
    storage.put(tx3, b"key3", b"val3").await.unwrap();
    storage.commit(tx3).await.unwrap();

    // 5. Force flush to create another SSTable with TX 3
    storage.force_flush().await.unwrap();

    // Verify all data is there
    assert_eq!(storage.get(b"key1").await.unwrap(), Some(b"val1".to_vec()));
    assert_eq!(storage.get(b"key2").await.unwrap(), Some(b"val2".to_vec()));
    assert_eq!(storage.get(b"key3").await.unwrap(), Some(b"val3".to_vec()));

    // 6. Rollback to TX 2
    storage.rollback_to_tx(tx2).await.unwrap();

    // 7. Verify TX 1 and 2 are still there, but TX 3 is GONE
    assert_eq!(storage.get(b"key1").await.unwrap(), Some(b"val1".to_vec()));
    assert_eq!(storage.get(b"key2").await.unwrap(), Some(b"val2".to_vec()));

    // THIS IS EXPECTED TO FAIL BEFORE THE FIX
    assert_eq!(
        storage.get(b"key3").await.unwrap(),
        None,
        "Data from TX 3 should be gone after rollback to TX 2"
    );
}
