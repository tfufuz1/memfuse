use memfuse_checkpoint::CheckpointManager;
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use std::sync::Arc;
use tempfile::tempdir;
#[tokio::test]
async fn test_checkpoint() {
    let tmp = tempdir().unwrap(); // unwrap
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        ..Default::default()
    };
    let storage = Arc::new(LsmStorage::new(config).await.unwrap()); // unwrap
    let manager = CheckpointManager::new(storage);
    manager
        .create_checkpoint("cp", "c", 1, serde_json::json!({}))
        .await
        .unwrap(); // unwrap
    assert_eq!(manager.list_checkpoints().await.unwrap().len(), 1); // unwrap
}
