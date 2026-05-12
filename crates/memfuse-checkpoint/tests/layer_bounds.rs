use memfuse_checkpoint::CheckpointManager;
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use std::sync::Arc;
use tempfile::TempDir;

// ANCHOR:TEST:LAYER-001 — Interaction between memfuse-checkpoint and memfuse-store.
#[tokio::test]
async fn test_layer_001_checkpoint_persistence() {
    let tmp = TempDir::new().unwrap();
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        ..Default::default()
    };
    let storage = Arc::new(LsmStorage::new(config).await.unwrap());
    let manager = CheckpointManager::new(storage.clone());

    let cp = manager.create_checkpoint("test_cp").await.unwrap();
    assert_eq!(cp.name, "test_cp");

    // In a real scenario, we'd verify it prevents GC or allows rollback.
    // Rollback is currently a stub in CheckpointManager.
    manager
        .rollback(&cp)
        .await
        .expect("rollback stub should return Ok");

    manager.drop_checkpoint(&cp).await.unwrap();
}
