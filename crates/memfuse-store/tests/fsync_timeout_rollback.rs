// FILE-CONTEXT: Integration test verifying fsync timeout handling, clean rollback, and intent file recovery. (TS: 2026-09-11)

use memfuse_core::{StorageEngine, TxId};
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use std::time::Duration;
use tempfile::tempdir;
use tokio::time::timeout;

#[tokio::test]
#[ignore = "Requires JULES-01 merge — tracks NC-3/C-4 intent recovery"]
async fn test_fsync_timeout_triggers_clean_rollback() {
    let dir = tempdir().expect("tempdir creation failed");
    let path = dir.path().to_path_buf();

    let config = LsmConfig {
        path: path.clone(),
        ..Default::default()
    };

    let storage = LsmStorage::new(config)
        .await
        .expect("LsmStorage creation failed");

    // 1. Commit baseline transaction tx1
    let tx1 = TxId::new(1);
    storage
        .put(tx1, b"key_base", b"val_base")
        .await
        .expect("put tx1 failed");
    storage.commit(tx1).await.expect("commit tx1 failed");

    // 2. Stage transaction tx2 and attempt commit wrapped in a very short timeout
    let tx2 = TxId::new(2);
    storage
        .put(tx2, b"key_timeout", b"val_timeout")
        .await
        .expect("put tx2 failed");

    // Simulate an fsync timeout by imposing a 0ms / 1ns timeout on commit
    let commit_res = timeout(Duration::from_nanos(1), storage.commit(tx2)).await;

    // (a) Verify timeout occurred
    assert!(
        commit_res.is_err(),
        "commit() wrapped in artificial timeout must time out"
    );

    // Drop current storage handle to simulate restart/recovery
    drop(storage);

    // (b) Reopen storage and verify state consistency and recovery
    let config_reopen = LsmConfig {
        path: path.clone(),
        ..Default::default()
    };
    let storage_reopen = LsmStorage::new(config_reopen)
        .await
        .expect("Reopening LsmStorage must succeed after fsync timeout recovery");

    // Verify baseline key is intact and uncommitted tx2 is absent
    let val_base = storage_reopen
        .get(b"key_base")
        .await
        .expect("get baseline key failed");
    assert_eq!(
        val_base,
        Some(b"val_base".to_vec()),
        "Baseline key must remain intact"
    );

    let val_timeout = storage_reopen
        .get(b"key_timeout")
        .await
        .expect("get timeout key failed");
    assert_eq!(
        val_timeout, None,
        "Timed-out transaction key must NOT exist after recovery"
    );
}
