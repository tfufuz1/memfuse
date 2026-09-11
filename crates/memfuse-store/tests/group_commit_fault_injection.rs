use memfuse_core::{MemFuseError, StorageEngine, TxId};
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_group_commit_fault_injection_all_participants_fail() {
    let tmp = TempDir::new().expect("temp dir");
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        memtable_size_limit: 64 * 1024 * 1024,
        max_ram_mb: 512,
        group_commit_window_micros: 10_000, // 10ms window to gather all tasks in a single batch
        ..Default::default()
    };

    let storage = Arc::new(
        LsmStorage::new(config.clone())
            .await
            .expect("create storage"),
    );

    // First commit tx 1 successfully so there is baseline state
    let tx1 = TxId::new(1);
    storage
        .put(tx1, b"base_k", b"base_v")
        .await
        .expect("put base");
    storage.commit(tx1).await.expect("commit base");

    // Replace WAL file handle with read-only handle to force append_batch failure
    storage.simulate_wal_append_failure_for_test().await;

    // Now spawn 10 concurrent commit tasks in parallel
    let num_tasks = 10u64;
    let mut set = tokio::task::JoinSet::new();

    for i in 2..=num_tasks + 1 {
        let st = Arc::clone(&storage);
        set.spawn(async move {
            let tx = TxId::new(i);
            let key = format!("fail_key_{:04}", i).into_bytes();
            let val = format!("fail_val_{:04}", i).into_bytes();
            st.put(tx, &key, &val).await.expect("put fail key");
            let commit_res = st.commit(tx).await;
            (i, commit_res)
        });
    }

    let mut fail_count = 0;
    while let Some(res) = set.join_next().await {
        let (_task_id, result) = res.expect("join task");
        assert!(
            result.is_err(),
            "ALL participants in batch must receive Err when WAL append fails"
        );
        if let Err(MemFuseError::Storage(err_msg)) = result {
            assert!(
                err_msg.contains("Commit failed (at WAL append)"),
                "Error message should indicate WAL append failure, got: {}",
                err_msg
            );
        } else {
            panic!("Expected MemFuseError::Storage, got {:?}", result);
        }
        fail_count += 1;
    }

    assert_eq!(fail_count, 10, "All 10 tasks must fail");

    // Restore WAL write permissions and verify storage state is uncorrupted
    storage.restore_wal_file_handle_for_test().await;

    // Baseline key must still exist
    let base_val = storage
        .get(b"base_k")
        .await
        .expect("get base")
        .expect("value exists");
    assert_eq!(&base_val, b"base_v");

    // Failed batch keys must NOT exist in storage
    for i in 2..=num_tasks + 1 {
        let key = format!("fail_key_{:04}", i).into_bytes();
        let val = storage.get(&key).await.expect("get fail key");
        assert_eq!(
            val, None,
            "Failed batch entry must not be present in storage"
        );
    }

    // Verify subsequent commit works cleanly after restoring WAL handle
    let tx_new = TxId::new(100);
    storage
        .put(tx_new, b"new_k", b"new_v")
        .await
        .expect("put new");
    storage.commit(tx_new).await.expect("commit new");

    let new_val = storage
        .get(b"new_k")
        .await
        .expect("get new")
        .expect("new value exists");
    assert_eq!(&new_val, b"new_v");
}
