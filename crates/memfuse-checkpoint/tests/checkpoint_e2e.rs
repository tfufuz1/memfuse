//! E2E Checkpoint Tests for MemFuse.
//!
//! ANCHOR:TEST:CHECKPOINT-E2E-001 STATUS:READY AGENT:12

use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use memfuse_checkpoint::CheckpointManager;
use tempfile::TempDir;
use std::sync::Arc;

#[tokio::test]
async fn test_checkpoint_facade_integration() {
    let tmp = TempDir::new().expect("Failed to create temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };

    // 1. Setup DB via facade
    let db = MemFuse::open_with_config(tmp.path(), config).await.expect("Failed to open DB");

    // 2. Use CheckpointManager with inner storage
    let storage = db.inner_storage();
    let manager = CheckpointManager::new(storage);

    // 3. Create Checkpoint
    let cp = manager.create_checkpoint("v1-stable").await.expect("Failed to create checkpoint");
    assert_eq!(cp.name, "v1-stable");

    // 4. Drop Checkpoint
    manager.drop_checkpoint(&cp).await.expect("Failed to drop checkpoint");
}
