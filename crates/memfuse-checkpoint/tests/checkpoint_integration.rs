//! E2E Integration tests for CheckpointManager and LsmStorage.
// ANCHOR:INTEGRATION:CHECKPOINT-E2E STATUS:READY AGENT:12 DATE:2026-06-20

use memfuse_checkpoint::CheckpointManager;
use memfuse_core::{StorageEngine, TxId};
use memfuse_store::{LsmConfig, LsmStorage};
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_checkpoint_manager_lsm_integration() {
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

    // 1. Insert some data
    let tx1 = TxId::new(1);
    storage.put(tx1, b"key1", b"val1").await.expect("put 1");
    storage.commit(tx1).await.expect("commit 1");

    let last_seq = storage.last_seq_no().await.expect("get last seq");

    // 2. Create checkpoint
    let cp_name = "v1";
    let meta = manager
        .create_checkpoint(cp_name, "default", last_seq, serde_json::json!({"ver": 1}))
        .await
        .expect("create checkpoint failed");

    assert_eq!(meta.name, cp_name);
    assert_eq!(meta.seq_no, last_seq);

    // 3. Add more data
    let tx2 = TxId::new(2);
    storage.put(tx2, b"key2", b"val2").await.expect("put 2");
    storage.commit(tx2).await.expect("commit 2");

    // 4. Verify both keys are present
    assert_eq!(storage.get(b"key1").await.unwrap(), Some(b"val1".to_vec()));
    assert_eq!(storage.get(b"key2").await.unwrap(), Some(b"val2".to_vec()));

    // 5. Verify time-travel (reading from checkpoint seq_no)
    assert_eq!(
        storage.get_at_seq(b"key1", last_seq).await.unwrap(),
        Some(b"val1".to_vec())
    );
    assert_eq!(storage.get_at_seq(b"key2", last_seq).await.unwrap(), None);

    // 6. Delete checkpoint and verify unpinning (indirectly by checking if we can still list it)
    manager.drop_checkpoint(cp_name).await.expect("drop failed");
    let list = manager.list_checkpoints().await.expect("list failed");
    assert!(list.iter().find(|c| c.name == cp_name).is_none());
}

#[tokio::test]
async fn test_checkpoint_persistence_across_instances() {
    let tmp = TempDir::new().expect("failed to create temp dir");
    let lsm_config = LsmConfig {
        path: tmp.path().to_path_buf(),
        ..Default::default()
    };

    {
        let storage = Arc::new(
            LsmStorage::new(lsm_config.clone())
                .await
                .expect("failed to open storage"),
        );
        let manager = CheckpointManager::new(storage.clone());

        let tx = TxId::new(1);
        storage.put(tx, b"p-key", b"p-val").await.expect("put");
        storage.commit(tx).await.expect("commit");
        let seq = storage.last_seq_no().await.expect("last seq");

        manager
            .create_checkpoint("persistent-cp", "default", seq, serde_json::json!({}))
            .await
            .expect("create failed");

        // Ensure data is flushed so it's persisted in LSM as checkpoint metadata
        storage.flush().await.expect("flush failed");
    }

    // New instance
    {
        let storage = Arc::new(
            LsmStorage::new(lsm_config)
                .await
                .expect("failed to re-open storage"),
        );
        let manager = CheckpointManager::new(storage.clone());

        let checkpoints = manager.list_checkpoints().await.expect("list failed");
        assert_eq!(checkpoints.len(), 1);
        assert_eq!(checkpoints[0].name, "persistent-cp");
    }
}
