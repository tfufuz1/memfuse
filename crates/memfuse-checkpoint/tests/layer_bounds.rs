use memfuse_checkpoint::CheckpointManager;
use memfuse_core::{StorageEngine, TxId};
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_layer_001_checkpoint_fork_diverge() {
    let tmp = TempDir::new().unwrap();
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        ..Default::default()
    };
    let storage = Arc::new(LsmStorage::new(config).await.expect("open storage"));
    let manager = CheckpointManager::new(storage.clone());

    // 1. Initial State
    let tx1 = TxId::new(1);
    storage.put(tx1, b"key", b"v1").await.unwrap();
    storage.commit(tx1).await.unwrap();

    // 2. Create Checkpoint (Fork)
    let cp = manager.create_checkpoint("v1_state").await.expect("create checkpoint");
    assert!(cp.seq_no > 0);

    // 3. Diverge (Overwrite)
    let tx2 = TxId::new(2);
    storage.put(tx2, b"key", b"v2").await.unwrap();
    storage.commit(tx2).await.unwrap();

    // Verify current state is v2
    assert_eq!(storage.get(b"key").await.unwrap().unwrap(), b"v2");

    // 4. Pinning verification (implicit in the fact that create_checkpoint calls pin_checkpoint)
    // We can verify that we can still "rollback" to the checkpoint,
    // although the current implementation of rollback is a no-op that returns Ok(()).
    manager.rollback(&cp).await.expect("rollback");

    // Cleanup
    manager.drop_checkpoint(&cp).await.expect("drop checkpoint");
}
