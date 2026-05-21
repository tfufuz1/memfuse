//! End-to-End checkpoint durability tests.
// ANCHOR:INTEGRATION:CHECKPOINT-E2E STATUS:READY AGENT:12 DATE:2026-05-21

use memfuse_checkpoint::CheckpointManager;
use memfuse_core::{StorageEngine, TxId};
use memfuse_store::{LsmConfig, LsmStorage};
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_checkpoint_pinning_across_restarts() {
    let tmp = TempDir::new().expect("failed to create temp dir");
    let db_path = tmp.path().to_path_buf();

    // 1. Session 1: Create data and checkpoint
    let seq_no = {
        let lsm_config = LsmConfig {
            path: db_path.clone(),
            ..Default::default()
        };
        let storage = Arc::new(LsmStorage::new(lsm_config).await.expect("storage"));
        let manager = CheckpointManager::new(storage.clone());

        let tx = TxId::new(1);
        storage.put(tx, b"key1", b"val1").await.expect("put");
        storage.commit(tx).await.expect("commit");
        storage.force_flush().await.expect("flush");

        let cp = manager
            .create_checkpoint("stable-v1")
            .await
            .expect("create cp");
        cp.seq_no
    };

    // 2. Session 2: Verify pinning is durable
    {
        let lsm_config = LsmConfig {
            path: db_path.clone(),
            ..Default::default()
        };
        let storage = Arc::new(LsmStorage::new(lsm_config).await.expect("storage"));

        // Verify we can still read data
        let val = storage.get(b"key1").await.expect("get").expect("missing");
        assert_eq!(val, b"val1");

        // Unpin manually via storage as CheckpointManager doesn't persist Checkpoint objects
        storage.unpin_checkpoint(seq_no).await.expect("unpin");
    }
}
