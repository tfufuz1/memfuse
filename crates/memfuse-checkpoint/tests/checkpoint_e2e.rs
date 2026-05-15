//! E2E Checkpoint Tests for MemFuse.
//!
//! ANCHOR:TEST:CHECKPOINT-E2E-001 STATUS:READY AGENT:12

use memfuse_db::{MemFuse, MemFuseConfig};
use memfuse_checkpoint::CheckpointManager;
use memfuse_store::lsm::{LsmStorage, LsmConfig};
use tempfile::TempDir;
use std::sync::Arc;

#[tokio::test]
async fn test_checkpoint_integration_lifecycle() {
    let tmp = TempDir::new().expect("Failed to create temp dir");
    let db_path = tmp.path().to_owned();
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };

    // 1. Setup DB via facade and insert some data
    {
        let db = MemFuse::open_with_config(&db_path, config).await.expect("Failed to open DB");
        db.insert("doc-1", &[1.0, 0.0, 0.0, 0.0], None).await.unwrap();
        // Drop db to release locks
    }

    // 2. Create Checkpoint using CheckpointManager on the same path
    {
        let lsm_config = LsmConfig {
            path: db_path.clone(),
            ..Default::default()
        };
        let storage = Arc::new(LsmStorage::new(lsm_config).await.expect("Failed to open storage"));
        let manager = CheckpointManager::new(storage);

        let cp = manager.create_checkpoint("v1-stable").await.expect("Failed to create checkpoint");
        assert_eq!(cp.name, "v1-stable");

        manager.drop_checkpoint(&cp).await.expect("Failed to drop checkpoint");
    }
}
