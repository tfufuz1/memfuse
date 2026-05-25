//! Integration tests for CheckpointManager using real LsmStorage.
// ANCHOR:INTEGRATION:CHECKPOINT-001 STATUS:DONE AGENT:12 DATE:2026-06-20

use memfuse_checkpoint::CheckpointManager;
use memfuse_core::{StorageEngine, TxId};
use memfuse_store::{LsmConfig, LsmStorage};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_checkpoint_lifecycle_with_real_storage() {
    let tmp = TempDir::new().expect("failed to create temp dir");
    let lsm_config = LsmConfig {
        path: tmp.path().to_path_buf(),
        ..Default::default()
    };

    // 1. Initialize real storage and manager
    let storage = Arc::new(
        LsmStorage::new(lsm_config)
            .await
            .expect("failed to open storage"),
    );
    let manager = CheckpointManager::new(storage.clone());

    // 2. Insert some data to have a sequence number > 0
    let tx1 = TxId::new(1);
    storage.put(tx1, b"key1", b"val1").await.expect("put failed");
    storage.commit(tx1).await.expect("commit failed");

    let seq = storage.last_seq_no().await.expect("get last seq");
    assert!(seq > 0);

    // 3. Create checkpoint
    let cp_name = "integration-v1";
    let metadata = json!({"version": 1, "description": "initial state"});
    let cp = manager
        .create_checkpoint(cp_name, "default", seq, metadata.clone())
        .await
        .expect("create checkpoint failed");

    assert_eq!(cp.name, cp_name);
    assert_eq!(cp.seq_no, seq);
    assert_eq!(cp.metadata, metadata);

    // 4. Verify it's in the list
    let list = manager.list_checkpoints().await.expect("list failed");
    assert!(list.iter().any(|c| c.name == cp_name));

    // 5. Verify retrieval
    let retrieved = manager.get_checkpoint(cp_name).await.expect("get failed").expect("missing cp");
    assert_eq!(retrieved.seq_no, seq);

    // 6. Test persistence across manager reloads
    // Note: Since LsmStorage holds a lock on the directory, we need to ensure the old manager/storage
    // are handled correctly if we were to recreate them.
    // But here we can just create a new manager instance sharing the SAME storage instance.
    let manager2 = CheckpointManager::new(storage.clone());
    let list2 = manager2.list_checkpoints().await.expect("list2 failed");
    assert!(list2.iter().any(|c| c.name == cp_name));

    // 7. Drop checkpoint
    manager.drop_checkpoint(cp_name).await.expect("drop failed");
    let list_after = manager.list_checkpoints().await.expect("list after failed");
    assert!(!list_after.iter().any(|c| c.name == cp_name));
}

#[tokio::test]
async fn test_checkpoint_persistence_db_restart() {
    let tmp = TempDir::new().expect("failed to create temp dir");
    let db_path = tmp.path().to_path_buf();
    let cp_name = "restart-test";

    // 1. Create data and checkpoint, then close everything
    {
        let lsm_config = LsmConfig {
            path: db_path.clone(),
            ..Default::default()
        };
        let storage = Arc::new(LsmStorage::new(lsm_config).await.expect("open storage 1"));
        let manager = CheckpointManager::new(storage.clone());

        let tx = TxId::new(1);
        storage.put(tx, b"persist-key", b"persist-val").await.unwrap();
        storage.commit(tx).await.unwrap();
        let seq = storage.last_seq_no().await.unwrap();

        manager.create_checkpoint(cp_name, "default", seq, json!({})).await.expect("create cp");

        // Dropping storage and manager releases locks
    }

    // 2. Re-open and verify checkpoint is still there
    {
        let lsm_config = LsmConfig {
            path: db_path,
            ..Default::default()
        };
        let storage = Arc::new(LsmStorage::new(lsm_config).await.expect("open storage 2"));
        let manager = CheckpointManager::new(storage.clone());

        // Trigger reload (since get_checkpoint doesn't auto-reload from storage in current scaffold)
        manager.list_checkpoints().await.expect("list failed after restart");

        let cp = manager.get_checkpoint(cp_name).await.expect("get cp after restart").expect("cp missing after restart");
        assert_eq!(cp.name, cp_name);

        let val = storage.get(b"persist-key").await.expect("get data").expect("data missing");
        assert_eq!(val, b"persist-val");
    }
}
