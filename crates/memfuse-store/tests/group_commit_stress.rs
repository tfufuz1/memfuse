use memfuse_core::{StorageEngine, TxId};
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use std::sync::Arc;
use std::time::Instant;
use tempfile::TempDir;

#[tokio::test]
async fn test_group_commit_concurrency_stress_200_tasks() {
    let tmp = TempDir::new().expect("temp dir");
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        memtable_size_limit: 64 * 1024 * 1024,
        max_ram_mb: 512,
        group_commit_window_micros: 500,
        ..Default::default()
    };

    let storage = Arc::new(LsmStorage::new(config.clone()).await.expect("create storage"));
    let num_tasks = 200u64;

    let start_time = Instant::now();
    let mut set = tokio::task::JoinSet::new();

    for i in 1..=num_tasks {
        let st = Arc::clone(&storage);
        set.spawn(async move {
            let tx = TxId::new(i);
            let key = format!("gc_key_{:04}", i).into_bytes();
            let val = format!("gc_val_{:04}", i).into_bytes();
            st.put(tx, &key, &val).await.expect("put");
            st.commit(tx).await.expect("commit");
            (key, val)
        });
    }

    let mut written = Vec::new();
    while let Some(res) = set.join_next().await {
        let (k, v) = res.expect("task panicked");
        written.push((k, v));
    }

    let elapsed = start_time.elapsed();
    println!(
        "Group-Commit Stress: 200 commits finished in {:?}",
        elapsed
    );

    assert_eq!(written.len(), 200);

    // Verify all keys are readable from in-memory storage
    for (k, v) in &written {
        let read_val = storage.get(k).await.expect("get").expect("value exists");
        assert_eq!(&read_val, v);
    }

    drop(storage);

    // Reopen storage and verify byte-for-byte replay equality
    let storage_reopened = LsmStorage::new(config).await.expect("reopen storage");
    for (k, v) in &written {
        let read_val = storage_reopened
            .get(k)
            .await
            .expect("get after reopen")
            .expect("value exists after reopen");
        assert_eq!(&read_val, v);
    }
}
