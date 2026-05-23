//! Integration tests for CheckpointManager with real LsmStorage.
// ANCHOR:INTEGRATION:CHECKPOINT-001 STATUS:DONE AGENT:12 DATE:2026-06-21

use memfuse_checkpoint::CheckpointManager;
use memfuse_store::{LsmConfig, LsmStorage};
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_checkpoint_persistence_real_storage() {
    let tmp = TempDir::new().expect("failed to create temp dir");
    let lsm_config = LsmConfig {
        path: tmp.path().to_path_buf(),
        ..Default::default()
    };

    let storage = Arc::new(
        LsmStorage::new(lsm_config.clone())
            .await
            .expect("failed to open storage"),
    );
    let manager = CheckpointManager::new(storage.clone());

    // 1. Create a checkpoint
    let metadata = serde_json::json!({"version": "1.0", "notes": "initial snapshot"});
    let cp = manager
        .create_checkpoint("integration-test", "coll-1", 42, metadata.clone())
        .await
        .expect("failed to create checkpoint");

    assert_eq!(cp.name, "integration-test");
    assert_eq!(cp.seq_no, 42);
    assert_eq!(cp.metadata, metadata);

    // 2. Drop the manager and storage, then reopen
    drop(manager);
    drop(storage);

    let storage2 = Arc::new(
        LsmStorage::new(lsm_config)
            .await
            .expect("failed to reopen storage"),
    );
    let manager2 = CheckpointManager::new(storage2);

    // 3. Verify checkpoint is still there
    let list = manager2
        .list_checkpoints()
        .await
        .expect("failed to list checkpoints");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "integration-test");
    assert_eq!(list[0].seq_no, 42);
    assert_eq!(list[0].metadata, metadata);

    let retrieved = manager2
        .get_checkpoint("integration-test")
        .await
        .unwrap()
        .expect("checkpoint not found");
    assert_eq!(retrieved, list[0]);

    // 4. Drop checkpoint
    manager2
        .drop_checkpoint("integration-test")
        .await
        .expect("failed to drop");
    let list_after = manager2.list_checkpoints().await.expect("failed to list");
    assert!(list_after.is_empty());
}
