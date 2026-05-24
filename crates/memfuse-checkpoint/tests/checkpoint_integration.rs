// ANCHOR:INTEGRATION:CHECKPOINT-001 STATUS:READY AGENT:12
use memfuse_checkpoint::CheckpointManager;
use memfuse_store::{LsmConfig, LsmStorage};
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_checkpoint_with_lsm_storage() {
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

    let meta = manager
        .create_checkpoint("real_lsm_cp", "col1", 123, serde_json::json!({"test": true}))
        .await
        .expect("failed to create checkpoint");

    assert_eq!(meta.name, "real_lsm_cp");
    assert_eq!(meta.seq_no, 123);

    let retrieved = manager
        .get_checkpoint("real_lsm_cp")
        .await
        .expect("failed to get checkpoint")
        .expect("checkpoint not found");
    assert_eq!(retrieved, meta);

    manager.drop_checkpoint("real_lsm_cp").await.expect("failed to drop checkpoint");
    let none = manager.get_checkpoint("real_lsm_cp").await.expect("failed to get after drop");
    assert!(none.is_none());
}
