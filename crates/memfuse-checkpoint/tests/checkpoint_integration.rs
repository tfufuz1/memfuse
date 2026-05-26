//! Integration tests for memfuse-checkpoint using real LsmStorage.
// ANCHOR:INTEGRATION:CHECKPOINT-001 STATUS:READY AGENT:12

use memfuse_checkpoint::CheckpointManager;
use memfuse_store::{LsmConfig, LsmStorage};
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_checkpoint_manager_with_real_storage() {
    let tmp = TempDir::new().expect("Failed to create temp dir");
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        ..Default::default()
    };

    let storage = Arc::new(
        LsmStorage::new(config.clone())
            .await
            .expect("Failed to open storage"),
    );
    let manager = CheckpointManager::new(storage.clone());

    // 1. Create a checkpoint
    let meta = manager
        .create_checkpoint("cp1", "coll1", 10, serde_json::json!({"state": "initial"}))
        .await
        .expect("Failed to create checkpoint");

    assert_eq!(meta.name, "cp1");
    assert_eq!(meta.seq_no, 10);

    // 2. Verify persistence (reload manager)
    let manager2 = CheckpointManager::new(storage.clone());
    let list = manager2
        .list_checkpoints()
        .await
        .expect("Failed to list checkpoints");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "cp1");
    assert_eq!(list[0].seq_no, 10);

    // 3. Verify pinning by creating a new manager with a new storage instance on same path
    // IMPORTANT: CheckpointManager uses a separate persistent_checkpoints cache.
    // It only reloads from storage if the cache is empty.
    let storage3 = Arc::new(
        LsmStorage::new(config.clone())
            .await
            .expect("Failed to reopen storage"),
    );
    let manager3 = CheckpointManager::new(storage3.clone());

    // list_checkpoints should trigger a reload if cache is empty
    let list_reloaded = manager3
        .list_checkpoints()
        .await
        .expect("Failed to list reloaded checkpoints");
    assert_eq!(list_reloaded.len(), 1);
    assert_eq!(list_reloaded[0].name, "cp1");

    let cp = manager3
        .get_checkpoint("cp1")
        .await
        .expect("Failed to get checkpoint")
        .expect("Checkpoint missing");
    assert_eq!(cp.seq_no, 10);

    // 4. Drop checkpoint and verify unpinned
    manager3
        .drop_checkpoint("cp1")
        .await
        .expect("Failed to drop checkpoint");
    let list_after = manager3
        .list_checkpoints()
        .await
        .expect("Failed to list checkpoints after drop");
    assert!(list_after.is_empty());
}

#[tokio::test]
async fn test_checkpoint_concurrency() {
    let tmp = TempDir::new().expect("Failed to create temp dir");
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        ..Default::default()
    };

    let storage = Arc::new(
        LsmStorage::new(config)
            .await
            .expect("Failed to open storage"),
    );
    let manager = Arc::new(CheckpointManager::new(storage.clone()));

    let mut handles = vec![];
    for i in 0..5 {
        let mgr = manager.clone();
        handles.push(tokio::spawn(async move {
            let name = format!("cp_{}", i);
            mgr.create_checkpoint(&name, "coll1", i as u64, serde_json::json!({}))
                .await
                .expect("Concurrent create failed");
        }));
    }

    for h in handles {
        h.await.expect("Task failed");
    }

    let list = manager.list_checkpoints().await.expect("Final list failed");
    assert_eq!(list.len(), 5);
}
