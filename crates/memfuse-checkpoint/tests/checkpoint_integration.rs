use memfuse_checkpoint::CheckpointManager;
use memfuse_store::{LsmConfig, LsmStorage};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;

// ANCHOR:INTEGRATION:CHECKPOINT-E2E STATUS:DONE AGENT:12 DATE:2026-06-21
// E2E test for CheckpointManager using real LsmStorage.

async fn setup_storage(path: &std::path::Path) -> Arc<LsmStorage> {
    let config = LsmConfig {
        path: path.to_path_buf(),
        ..Default::default()
    };
    Arc::new(LsmStorage::new(config).await.expect("Failed to open storage"))
}

#[tokio::test]
async fn test_checkpoint_manager_e2e() {
    let tmp = TempDir::new().expect("failed to create temp dir");
    let storage = setup_storage(tmp.path()).await;
    let manager = CheckpointManager::new(storage.clone());

    // 1. Create a checkpoint
    let meta = manager
        .create_checkpoint("v1", "coll_1", 10, json!({"purpose": "test"}))
        .await
        .expect("Failed to create checkpoint");

    assert_eq!(meta.name, "v1");
    assert_eq!(meta.collection_id, "coll_1");
    assert_eq!(meta.seq_no, 10);

    // 2. List checkpoints
    let list = manager.list_checkpoints().await.expect("Failed to list checkpoints");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "v1");

    // 3. Get checkpoint
    let retrieved = manager.get_checkpoint("v1").await.expect("Failed to get checkpoint");
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().metadata, json!({"purpose": "test"}));

    // 4. Drop checkpoint
    manager.drop_checkpoint("v1").await.expect("Failed to drop checkpoint");
    let list_after_drop = manager.list_checkpoints().await.expect("Failed to list checkpoints");
    assert!(list_after_drop.is_empty());
}

#[tokio::test]
async fn test_checkpoint_manager_persistence() {
    let tmp = TempDir::new().expect("failed to create temp dir");
    let db_path = tmp.path().to_path_buf();

    // Manager 1: Create checkpoint
    {
        let storage = setup_storage(&db_path).await;
        let manager = CheckpointManager::new(storage.clone());
        manager
            .create_checkpoint("persist_test", "c1", 100, json!({}))
            .await
            .expect("Failed to create checkpoint");
    }

    // Manager 2: Reload checkpoint from same path
    {
        let storage = setup_storage(&db_path).await;
        let manager = CheckpointManager::new(storage.clone());
        let list = manager.list_checkpoints().await.expect("Failed to list checkpoints");

        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "persist_test");
        assert_eq!(list[0].seq_no, 100);
    }
}

#[tokio::test]
async fn test_checkpoint_manager_concurrency() {
    let tmp = TempDir::new().expect("failed to create temp dir");
    let storage = setup_storage(tmp.path()).await;
    let manager = Arc::new(CheckpointManager::new(storage.clone()));

    let mut handles = Vec::new();
    for i in 0..10 {
        let manager = manager.clone();
        handles.push(tokio::spawn(async move {
            let name = format!("cp-{}", i);
            manager
                .create_checkpoint(&name, "coll", i as u64, json!({}))
                .await
                .expect("Concurrent create failed");
        }));
    }

    for h in handles {
        h.await.expect("Task panicked");
    }

    let list = manager.list_checkpoints().await.expect("Failed to list");
    assert_eq!(list.len(), 10);
}
