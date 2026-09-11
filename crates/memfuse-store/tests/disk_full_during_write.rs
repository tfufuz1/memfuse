#![cfg(feature = "fault-injection")]
// FILE-CONTEXT: Integration test verifying clean rollback and absence of phantom entries on WAL write failure (disk full simulation). (TS: 2026-09-11)

use memfuse_core::{StorageEngine, TxId};
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use memfuse_store::wal::FAIL_APPEND_FOR_TX;
use std::sync::atomic::Ordering;
use tempfile::tempdir;

#[tokio::test]
async fn test_disk_full_during_embedding_write_leaves_no_phantom_state() {
    let dir = tempdir().expect("tempdir creation failed");
    let config = LsmConfig {
        path: dir.path().to_path_buf(),
        ..Default::default()
    };

    let storage = LsmStorage::new(config)
        .await
        .expect("LsmStorage creation failed");

    // 1. Commit baseline transaction tx1
    let tx1 = TxId::new(1);
    storage
        .put(tx1, b"key_baseline", b"val_baseline")
        .await
        .expect("put tx1 failed");
    storage.commit(tx1).await.expect("commit tx1 failed");

    // Verify baseline key is readable
    assert_eq!(
        storage.get(b"key_baseline").await.expect("get baseline"),
        Some(b"val_baseline".to_vec())
    );

    // 2. Prepare transaction tx2 and activate fault injection simulating disk full / I/O error during WAL append
    let tx2 = TxId::new(2);
    storage
        .put(tx2, b"key_failed_tx", b"val_failed_tx")
        .await
        .expect("put tx2 failed");

    FAIL_APPEND_FOR_TX.store(tx2.inner(), Ordering::SeqCst);

    // Commit tx2 should fail due to simulated WAL append failure ("No space left on device" / fault injection)
    let commit_res = storage.commit(tx2).await;

    // (a) Verify caller receives a clear Err(...)
    assert!(
        commit_res.is_err(),
        "commit() must fail when WAL append fails due to disk full simulation"
    );
    let err_msg = commit_res.err().unwrap().to_string();
    assert!(
        err_msg.contains("WAL rollback executed") || err_msg.contains("fault injection"),
        "Error message should clearly indicate commit/WAL failure: {}",
        err_msg
    );

    // (b) Verify MemTable / Storage contains NO phantom entry for the failed transaction
    let val_failed = storage
        .get(b"key_failed_tx")
        .await
        .expect("get failed key");
    assert_eq!(
        val_failed, None,
        "MemTable must NOT contain phantom state for failed transaction tx2"
    );

    // (c) Verify a subsequent, successful commit tx3 functions correctly (system recovered)
    let tx3 = TxId::new(3);
    storage
        .put(tx3, b"key_recovery", b"val_recovery")
        .await
        .expect("put tx3 failed");
    storage
        .commit(tx3)
        .await
        .expect("subsequent commit tx3 must succeed after recovery");

    // Verify key_recovery is stored and baseline remains intact
    assert_eq!(
        storage.get(b"key_recovery").await.expect("get recovery"),
        Some(b"val_recovery".to_vec())
    );
    assert_eq!(
        storage.get(b"key_baseline").await.expect("get baseline"),
        Some(b"val_baseline".to_vec())
    );
}
