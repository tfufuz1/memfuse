use memfuse_checkpoint::CheckpointManager;
use memfuse_core::{StorageEngine, TxId};
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_checkpoint_lifecycle_e2e() {
    let tmp = TempDir::new().unwrap();
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        ..Default::default()
    };

    let storage = Arc::new(LsmStorage::new(config).await.unwrap());
    let manager = CheckpointManager::new(storage.clone());

    // 1. Initial State
    let tx = TxId::new(1);
    storage.put(tx, b"key1", b"val1").await.unwrap();
    storage.commit(tx).await.unwrap();

    // 2. Create Checkpoint
    let cp1 = manager.create_checkpoint("stable-state").await.unwrap();
    assert!(cp1.seq_no >= 1);

    // 3. More writes
    let tx2 = TxId::new(2);
    storage.put(tx2, b"key1", b"val2").await.unwrap();
    storage.commit(tx2).await.unwrap();

    // Create another checkpoint
    let cp2 = manager.create_checkpoint("later-state").await.unwrap();
    assert!(cp2.seq_no > cp1.seq_no);

    // 4. Drop Checkpoint
    manager.drop_checkpoint(&cp1).await.unwrap();

    // 5. Verify manager still works
    let cp3 = manager.create_checkpoint("final").await.unwrap();
    assert!(cp3.seq_no >= cp2.seq_no);
}
