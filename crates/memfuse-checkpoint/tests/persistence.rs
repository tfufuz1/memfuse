use memfuse_checkpoint::CheckpointManager;
use memfuse_core::StorageEngine;
use memfuse_store::{LsmConfig, LsmStorage};
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_checkpoint_persistence_with_real_lsm() {
    let tmp = TempDir::new().unwrap();
    let lsm_config = LsmConfig {
        path: tmp.path().to_path_buf(),
        ..Default::default()
    };

    // Session 1: Create a checkpoint
    {
        let storage = Arc::new(LsmStorage::new(lsm_config.clone()).await.unwrap());
        let manager = CheckpointManager::new(storage.clone());

        manager
            .create_checkpoint(
                "persist_test",
                "collection_1",
                123,
                serde_json::json!({"state": "active"}),
            )
            .await
            .unwrap();

        // Ensure data is on disk
        storage.flush().await.unwrap();
    }

    // Session 2: Reload and verify
    {
        let storage = Arc::new(LsmStorage::new(lsm_config).await.unwrap());
        let manager = CheckpointManager::new(storage);

        let list = manager.list_checkpoints().await.unwrap();
        assert_eq!(list.len(), 1, "Should have reloaded 1 checkpoint");
        assert_eq!(list[0].name, "persist_test");
        assert_eq!(list[0].seq_no, 123);
        assert_eq!(list[0].metadata["state"], "active");
    }
}

#[tokio::test]
async fn test_multiple_checkpoints_ordering() {
    let tmp = TempDir::new().unwrap();
    let lsm_config = LsmConfig {
        path: tmp.path().to_path_buf(),
        ..Default::default()
    };

    let storage = Arc::new(LsmStorage::new(lsm_config.clone()).await.unwrap());
    let manager = CheckpointManager::new(storage.clone());

    manager.create_checkpoint("cp2", "c1", 20, serde_json::json!({})).await.unwrap();
    manager.create_checkpoint("cp1", "c1", 10, serde_json::json!({})).await.unwrap();
    manager.create_checkpoint("cp3", "c1", 30, serde_json::json!({})).await.unwrap();

    let list = manager.list_checkpoints().await.unwrap();
    assert_eq!(list.len(), 3);
    assert_eq!(list[0].name, "cp1");
    assert_eq!(list[1].name, "cp2");
    assert_eq!(list[2].name, "cp3");
}
