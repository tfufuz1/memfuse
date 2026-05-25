// AGENT:12 DATE:2026-05-25 STATUS:READY
// ANCHOR:INTEGRATION:CHECKPOINT-001 — Checkpoint persistence with real LSM storage.

use memfuse_checkpoint::CheckpointManager;
use memfuse_store::lsm::{LsmStorage, LsmConfig};
use std::sync::Arc;
use tempfile::tempdir;

#[tokio::test]
async fn test_checkpoint_persistence_integration() {
    let tmp = tempdir().unwrap();
    let storage_path = tmp.path().join("lsm_storage");

    // 1. Setup real storage
    let config = LsmConfig {
        path: storage_path.clone(),
        ..Default::default()
    };
    let storage = Arc::new(LsmStorage::new(config).await.unwrap());
    let manager = CheckpointManager::new(storage.clone());

    let cp_name = "test-checkpoint-001";
    let metadata = serde_json::json!({"test": "data", "version": 1});

    // 2. Create checkpoint
    let meta = manager.create_checkpoint(
        cp_name,
        "collection_alpha",
        123,
        metadata.clone()
    ).await.expect("Failed to create checkpoint");

    assert_eq!(meta.name, cp_name);
    assert_eq!(meta.seq_no, 123);

    // 3. Close storage and manager
    drop(manager);
    drop(storage);

    // 4. Reopen and verify persistence
    let config_reopened = LsmConfig {
        path: storage_path,
        ..Default::default()
    };
    let storage_reopened = Arc::new(LsmStorage::new(config_reopened).await.unwrap());
    let manager_reopened = CheckpointManager::new(storage_reopened);

    // Trigger reload
    let list = manager_reopened.list_checkpoints().await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, cp_name);
    assert_eq!(list[0].metadata, metadata);

    let retrieved = manager_reopened.get_checkpoint(cp_name).await.unwrap().unwrap();
    assert_eq!(retrieved, list[0]);
}

#[tokio::test]
async fn test_checkpoint_drop_persistence_integration() {
    let tmp = tempdir().unwrap();
    let storage_path = tmp.path().join("lsm_storage_drop");

    let config = LsmConfig {
        path: storage_path.clone(),
        ..Default::default()
    };
    let storage = Arc::new(LsmStorage::new(config).await.unwrap());
    let manager = CheckpointManager::new(storage.clone());

    manager.create_checkpoint("cp1", "c1", 10, serde_json::json!({})).await.unwrap();
    manager.create_checkpoint("cp2", "c1", 20, serde_json::json!({})).await.unwrap();

    assert_eq!(manager.list_checkpoints().await.unwrap().len(), 2);

    manager.drop_checkpoint("cp1").await.unwrap();
    assert_eq!(manager.list_checkpoints().await.unwrap().len(), 1);

    drop(manager);
    drop(storage);

    let config_reopened = LsmConfig {
        path: storage_path,
        ..Default::default()
    };
    let storage_reopened = Arc::new(LsmStorage::new(config_reopened).await.unwrap());
    let manager_reopened = CheckpointManager::new(storage_reopened);

    let list = manager_reopened.list_checkpoints().await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "cp2");
}
