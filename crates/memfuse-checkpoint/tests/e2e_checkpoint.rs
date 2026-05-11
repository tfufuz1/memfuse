use memfuse_checkpoint::CheckpointManager;
use memfuse_core::{StorageEngine, TxId};
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_checkpoint_lifecycle_with_storage() {
    let tmp = TempDir::new().unwrap();
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        ..Default::default()
    };

    let storage = Arc::new(
        LsmStorage::new(config)
            .await
            .expect("Failed to create storage"),
    );
    let manager = CheckpointManager::new(Arc::clone(&storage));

    // 1. Insert some data
    let tx1 = TxId::new(1);
    storage.put(tx1, b"key1", b"val1").await.unwrap();
    storage.commit(tx1).await.unwrap();

    let initial_seq = storage.last_seq_no();

    // 2. Create checkpoint
    let cp = manager
        .create_checkpoint("v1")
        .await
        .expect("Failed to create checkpoint");
    assert_eq!(cp.name, "v1");
    assert_eq!(cp.seq_no, initial_seq);

    // 3. Modify data
    let tx2 = TxId::new(2);
    storage.put(tx2, b"key1", b"val2").await.unwrap();
    storage.commit(tx2).await.unwrap();

    assert!(storage.last_seq_no() > initial_seq);

    // 4. Rollback (currently a stub, but should return Ok)
    manager.rollback(&cp).await.expect("Rollback failed (stub)");

    // 5. Drop checkpoint
    manager
        .drop_checkpoint(&cp)
        .await
        .expect("Drop checkpoint failed");
}
