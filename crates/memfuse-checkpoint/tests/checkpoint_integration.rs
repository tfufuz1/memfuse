//! Integration tests for CheckpointManager with real LSM storage.
// ANCHOR:INTEGRATION:CHECKPOINT-001 STATUS:READY AGENT:12 DATE:2026-06-10

use memfuse_checkpoint::CheckpointManager;
use memfuse_core::{StorageEngine, TxId};
use memfuse_store::{LsmConfig, LsmStorage};
use serde_json::json;
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
        LsmStorage::new(config)
            .await
            .expect("Failed to create LSM storage"),
    );
    let manager = CheckpointManager::new(storage.clone());

    // 1. Insert some initial data
    let tx1 = TxId::new(1);
    storage
        .put(tx1, b"key1", b"value1")
        .await
        .expect("Put failed");
    storage.commit(tx1).await.expect("Commit failed");

    // 2. Create a checkpoint
    let meta = manager
        .create_checkpoint("cp1", "coll_1", 1, json!({"version": "1.0"}))
        .await
        .expect("Create checkpoint failed");

    assert_eq!(meta.name, "cp1");
    assert_eq!(meta.seq_no, 1);

    // 3. Verify checkpoint metadata is persisted and retrievable
    let retrieved = manager
        .get_checkpoint("cp1")
        .await
        .expect("Get failed")
        .expect("Not found");
    assert_eq!(retrieved.metadata["version"], "1.0");

    // 4. Reload from storage with a new manager instance
    let manager2 = CheckpointManager::new(storage.clone());
    let list = manager2.list_checkpoints().await.expect("List failed");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "cp1");

    // 5. Drop checkpoint and verify it's gone
    manager.drop_checkpoint("cp1").await.expect("Drop failed");
    let after_drop = manager.get_checkpoint("cp1").await.expect("Get failed");
    assert!(after_drop.is_none());

    // Verify it's also gone from storage (reloading manager2)
    manager2.reload_from_storage().await.expect("Reload failed");
    let list2 = manager2.list_checkpoints().await.expect("List failed");
    assert!(list2.is_empty());
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
            .expect("Failed to create LSM storage"),
    );
    let manager = Arc::new(CheckpointManager::new(storage.clone()));

    let mut tasks = Vec::new();
    for i in 0..10 {
        let mgr = manager.clone();
        tasks.push(tokio::spawn(async move {
            let name = format!("cp-{}", i);
            mgr.create_checkpoint(&name, "coll", i as u64, json!({"i": i}))
                .await
                .expect("Concurrent create failed");
        }));
    }

    for task in tasks {
        task.await.expect("Task failed");
    }

    let list = manager.list_checkpoints().await.expect("List failed");
    assert_eq!(list.len(), 10);
}
