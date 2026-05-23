use memfuse_checkpoint::CheckpointManager;
use memfuse_store::{LsmConfig, LsmStorage};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;

// ANCHOR:INTEGRATION:CHECKPOINT-001 STATUS:READY AGENT:12 DATE:2026-05-22
#[tokio::test]
async fn test_checkpoint_lifecycle_with_real_storage() {
    let tmp = TempDir::new().unwrap();
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        ..Default::default()
    };

    let storage = Arc::new(LsmStorage::new(config.clone()).await.unwrap());
    let manager = CheckpointManager::new(storage.clone());

    // 1. Create Checkpoint
    let cp_name = "integration_test_cp";
    let meta = manager
        .create_checkpoint(cp_name, "test_col", 10, json!({"version": 1}))
        .await
        .expect("Failed to create checkpoint");

    assert_eq!(meta.name, cp_name);
    assert_eq!(meta.seq_no, 10);

    // 2. List Checkpoints
    let list = manager
        .list_checkpoints()
        .await
        .expect("Failed to list checkpoints");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, cp_name);

    // 3. Drop storage and reload from new manager (Persistence Check)
    drop(manager);
    drop(storage);

    let storage2 = Arc::new(LsmStorage::new(config).await.unwrap());
    let manager2 = CheckpointManager::new(storage2);

    let list2 = manager2
        .list_checkpoints()
        .await
        .expect("Failed to list checkpoints after reload");
    assert_eq!(list2.len(), 1);
    assert_eq!(list2[0].name, cp_name);

    // 4. Drop Checkpoint
    manager2
        .drop_checkpoint(cp_name)
        .await
        .expect("Failed to drop checkpoint");
    let list3 = manager2
        .list_checkpoints()
        .await
        .expect("Failed to list checkpoints after drop");
    assert_eq!(list3.len(), 0);
}
