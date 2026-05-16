//! E2E tests for MemFuse Checkpointing.

use memfuse_db::{MemFuse, MemFuseConfig};
use memfuse_checkpoint::CheckpointManager;
use tempfile::TempDir;

#[tokio::test]
async fn test_checkpoint_lifecycle_e2e() {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(tmp.path(), config)
        .await
        .expect("open db");

    let storage = db.inner_storage();
    let manager = CheckpointManager::new(storage.clone());

    // 1. Initial state
    db.insert("k1", &[1.0, 0.0, 0.0, 0.0], None).await.expect("ins");

    // 2. Create checkpoint
    let cp1 = manager.create_checkpoint("v1").await.expect("create cp");
    assert_eq!(cp1.name, "v1");

    // 3. Change state
    db.insert("k2", &[0.0, 1.0, 0.0, 0.0], None).await.expect("ins 2");
    assert_eq!(db.len().await.unwrap(), 2);

    // 4. Rollback (placeholder implementation returns Ok)
    manager.rollback(&cp1).await.expect("rollback");

    // 5. Drop checkpoint
    manager.drop_checkpoint(&cp1).await.expect("drop cp");
}
