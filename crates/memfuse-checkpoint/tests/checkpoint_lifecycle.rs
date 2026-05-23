use memfuse_checkpoint::CheckpointManager;
use memfuse_core::{StorageEngine, TxId};
use memfuse_store::{LsmConfig, LsmStorage};
use std::sync::Arc;
use tempfile::TempDir;

// ANCHOR:INTEGRATION:CHECKPOINT-001 STATUS:DONE AGENT:12 DATE:2026-05-23
// Full lifecycle test for CheckpointManager using real LsmStorage.
#[tokio::test]
async fn test_checkpoint_lifecycle_full() {
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
    let manager = CheckpointManager::new(storage.clone());

    // 1. Create multiple checkpoints
    let meta1 = manager
        .create_checkpoint("cp1", "coll1", 10, serde_json::json!({"v": 1}))
        .await
        .expect("create cp1");
    let meta2 = manager
        .create_checkpoint("cp2", "coll1", 20, serde_json::json!({"v": 2}))
        .await
        .expect("create cp2");

    // 2. List and verify order
    let list = manager.list_checkpoints().await.expect("list");
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].name, "cp1");
    assert_eq!(list[1].name, "cp2");

    // 3. Reload from storage (simulating restart)
    let manager2 = CheckpointManager::new(storage.clone());
    let list2 = manager2.list_checkpoints().await.expect("list after restart");
    assert_eq!(list2.len(), 2);
    assert_eq!(list2[0], meta1);
    assert_eq!(list2[1], meta2);

    // 4. Drop a checkpoint
    manager.drop_checkpoint("cp1").await.expect("drop cp1");
    let list3 = manager.list_checkpoints().await.expect("list after drop");
    assert_eq!(list3.len(), 1);
    assert_eq!(list3[0].name, "cp2");

    // 5. Verify unpinned (internal check via storage stats or behavior)
    // For now we just verify it's gone from manager
    assert!(manager.get_checkpoint("cp1").await.expect("get").is_none());
}

#[tokio::test]
async fn test_checkpoint_storage_integration() {
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
    let manager = CheckpointManager::new(storage.clone());

    // Insert some data
    let tx = TxId::new(1);
    storage.put(tx, b"key1", b"val1").await.expect("put");
    storage.commit(tx).await.expect("commit");

    // Create checkpoint
    manager
        .create_checkpoint("stable", "default", 1, serde_json::json!({}))
        .await
        .expect("checkpoint");

    // Delete data
    let tx2 = TxId::new(2);
    storage.delete(tx2, b"key1").await.expect("delete");
    storage.commit(tx2).await.expect("commit");

    // Verify data is gone in current view
    assert!(storage.get(b"key1").await.expect("get").is_none());

    // Checkpoint meta still exists
    let cp = manager.get_checkpoint("stable").await.expect("get cp").unwrap();
    assert_eq!(cp.seq_no, 1);
}
