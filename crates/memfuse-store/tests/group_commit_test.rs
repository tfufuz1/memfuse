//! Group-Commit-Batching verification and benchmark tests for LsmStorage.

use memfuse_core::{MemFuseError, StorageEngine, TxId};
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tempfile::TempDir;

#[tokio::test]
async fn test_group_commit_200_parallel_tasks_durability_and_replay_parity() {
    let tmp = TempDir::new().expect("temp dir");
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        group_commit_window_micros: 500,
        ..Default::default()
    };

    let storage = Arc::new(LsmStorage::new(config.clone()).await.expect("new storage"));
    let mut set = tokio::task::JoinSet::new();

    // 200 parallel tasks calling commit() concurrently
    for i in 1..=200u64 {
        let storage_clone = Arc::clone(&storage);
        set.spawn(async move {
            let tx = TxId::new(i);
            let key = format!("gk_{:04}", i).into_bytes();
            let val = format!("gv_{:04}", i).into_bytes();

            storage_clone
                .put(tx, &key, &val)
                .await
                .expect("put succeeds");
            storage_clone
                .commit(tx)
                .await
                .expect("group commit succeeds");
        });
    }

    while let Some(res) = set.join_next().await {
        res.expect("task join succeeds");
    }

    // Verify in-memory state for all 200 keys
    for i in 1..=200u64 {
        let key = format!("gk_{:04}", i).into_bytes();
        let expected_val = format!("gv_{:04}", i).into_bytes();
        let val = storage.get(&key).await.expect("get succeeds");
        assert_eq!(
            val,
            Some(expected_val),
            "Key gk_{:04} missing or mismatch in active storage",
            i
        );
    }

    // Close original storage (flush active memtable to disk)
    storage.close().await.expect("close succeeds");

    // Reopen storage from disk and verify WAL/SSTable replay parity
    let reopened = LsmStorage::new(config).await.expect("reopen storage");
    for i in 1..=200u64 {
        let key = format!("gk_{:04}", i).into_bytes();
        let expected_val = format!("gv_{:04}", i).into_bytes();
        let val = reopened.get(&key).await.expect("get succeeds after reopen");
        assert_eq!(
            val,
            Some(expected_val),
            "Key gk_{:04} missing or mismatch after WAL/SSTable replay",
            i
        );
    }
}

#[cfg(feature = "fault-injection")]
#[tokio::test]
#[cfg(feature = "fault-injection")]
async fn test_group_commit_mid_batch_fsync_failure_atomicity() {
    use memfuse_store::wal::FAIL_APPEND_FOR_TX;

    let tmp = TempDir::new().expect("temp dir");
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        group_commit_window_micros: 2_000, // 2ms window to reliably collect multiple tasks into a single batch
        ..Default::default()
    };

    let storage = Arc::new(LsmStorage::new(config).await.expect("new storage"));

    // Stage 1: Commit base transaction 1 successfully
    let tx_base = TxId::new(1);
    storage
        .put(tx_base, b"base_key", b"base_val")
        .await
        .expect("put base");
    storage.commit(tx_base).await.expect("commit base");

    // Configure fault injection to fail append_batch for tx 10
    FAIL_APPEND_FOR_TX.store(10, Ordering::SeqCst);

    let barrier = Arc::new(tokio::sync::Barrier::new(10));
    let mut set = tokio::task::JoinSet::new();

    for i in 5..=14u64 {
        let storage_clone = Arc::clone(&storage);
        let barrier_clone = Arc::clone(&barrier);
        set.spawn(async move {
            let tx = TxId::new(i);
            let key = format!("batch_fail_key_{}", i).into_bytes();
            let val = format!("batch_fail_val_{}", i).into_bytes();

            storage_clone.put(tx, &key, &val).await.unwrap();
            barrier_clone.wait().await;

            storage_clone.commit(tx).await
        });
    }

    let mut err_count = 0;
    while let Some(res) = set.join_next().await {
        let commit_res = res.expect("join succeeds");
        if let Err(err) = commit_res {
            err_count += 1;
            assert!(
                matches!(err, MemFuseError::Storage(_)),
                "Expected MemFuseError::Storage, got: {:?}",
                err
            );
        }
    }

    // Reset fault injection flag
    FAIL_APPEND_FOR_TX.store(0, Ordering::SeqCst);

    // All 10 tasks in the batch must receive an Err (no partial success)
    assert_eq!(
        err_count, 10,
        "All 10 tasks in failed group commit batch must return Err"
    );

    // Verify storage state consistency: base key remains intact, failed keys are NOT visible
    let base_val = storage.get(b"base_key").await.expect("get base");
    assert_eq!(base_val, Some(b"base_val".to_vec()));

    for i in 5..=14u64 {
        let key = format!("batch_fail_key_{}", i).into_bytes();
        let val = storage.get(&key).await.expect("get failed key");
        assert_eq!(
            val, None,
            "Failed transaction key {} must NOT be visible in storage",
            i
        );
    }
}

#[tokio::test]
async fn test_group_commit_micro_benchmark_throughput_comparison() {
    let tmp_no_batch = TempDir::new().expect("temp dir");
    let tmp_batch = TempDir::new().expect("temp dir");

    let num_tasks = 40;
    let commits_per_task = 10;

    // 1. Measure single immediate commit (group_commit_window_micros = 0)
    let config_no_batch = LsmConfig {
        path: tmp_no_batch.path().to_path_buf(),
        group_commit_window_micros: 0,
        ..Default::default()
    };
    let storage_no_batch = Arc::new(LsmStorage::new(config_no_batch).await.expect("new storage"));

    let start_no_batch = Instant::now();
    let counter_no_batch = Arc::new(AtomicU64::new(1));
    let mut set_no_batch = tokio::task::JoinSet::new();

    for _ in 0..num_tasks {
        let storage_clone = Arc::clone(&storage_no_batch);
        let counter_clone = Arc::clone(&counter_no_batch);
        set_no_batch.spawn(async move {
            for _ in 0..commits_per_task {
                let tx_num = counter_clone.fetch_add(1, Ordering::SeqCst);
                let tx = TxId::new(tx_num);
                let key = format!("nobatch_k_{}", tx_num).into_bytes();
                let val = format!("nobatch_v_{}", tx_num).into_bytes();

                storage_clone.put(tx, &key, &val).await.unwrap();
                storage_clone.commit(tx).await.unwrap();
            }
        });
    }

    while let Some(res) = set_no_batch.join_next().await {
        res.unwrap();
    }
    let duration_no_batch = start_no_batch.elapsed();

    // 2. Measure group commit (group_commit_window_micros = 500)
    let config_batch = LsmConfig {
        path: tmp_batch.path().to_path_buf(),
        group_commit_window_micros: 500,
        ..Default::default()
    };
    let storage_batch = Arc::new(LsmStorage::new(config_batch).await.expect("new storage"));

    let start_batch = Instant::now();
    let counter_batch = Arc::new(AtomicU64::new(1));
    let mut set_batch = tokio::task::JoinSet::new();

    for _ in 0..num_tasks {
        let storage_clone = Arc::clone(&storage_batch);
        let counter_clone = Arc::clone(&counter_batch);
        set_batch.spawn(async move {
            for _ in 0..commits_per_task {
                let tx_num = counter_clone.fetch_add(1, Ordering::SeqCst);
                let tx = TxId::new(tx_num);
                let key = format!("batch_k_{}", tx_num).into_bytes();
                let val = format!("batch_v_{}", tx_num).into_bytes();

                storage_clone.put(tx, &key, &val).await.unwrap();
                storage_clone.commit(tx).await.unwrap();
            }
        });
    }

    while let Some(res) = set_batch.join_next().await {
        res.unwrap();
    }
    let duration_batch = start_batch.elapsed();

    let total_commits = num_tasks * commits_per_task;
    let tps_no_batch = (total_commits as f64) / duration_no_batch.as_secs_f64();
    let tps_batch = (total_commits as f64) / duration_batch.as_secs_f64();

    println!("\n=== GROUP COMMIT THROUGHPUT MICRO-BENCHMARK ===");
    println!("Total Commits: {}", total_commits);
    println!(
        "No Batching (0µs):   {:?} ({:.2} commits/sec)",
        duration_no_batch, tps_no_batch
    );
    println!(
        "Group Commit (500µs): {:?} ({:.2} commits/sec)",
        duration_batch, tps_batch
    );
    println!("Speedup Factor: {:.2}x\n", tps_batch / tps_no_batch);

    // Integrity check
    for i in 1..=total_commits as u64 {
        let key_nobatch = format!("nobatch_k_{}", i).into_bytes();
        let val_nobatch = storage_no_batch.get(&key_nobatch).await.unwrap();
        assert!(val_nobatch.is_some());

        let key_batch = format!("batch_k_{}", i).into_bytes();
        let val_batch = storage_batch.get(&key_batch).await.unwrap();
        assert!(val_batch.is_some());
    }
}
