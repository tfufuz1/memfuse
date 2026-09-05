//! Chaos task massacre integration test for concurrent writer task cancellation.
//!
//! Verifies atomic commit guarantees, snapshot integrity, and recovery determinism
//! under brutal asynchronous task abortion (`JoinHandle::abort()`).

use memfuse_core::{StorageEngine, TxId};
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use rand::seq::SliceRandom;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_chaos_task_massacre() {
    let temp_dir = TempDir::new().expect("temp dir creation failed");
    let config = LsmConfig {
        path: temp_dir.path().to_path_buf(),
        // Small memtable size limit to force active flushing and WAL rotation during chaos
        memtable_size_limit: 64 * 1024,
        ..Default::default()
    };

    let storage = Arc::new(
        LsmStorage::new(config.clone())
            .await
            .expect("LsmStorage creation failed"),
    );

    let num_tasks = 50;
    let items_per_task = 20;

    // 1. Record ground-truth for all planned (Key, Value) pairs outside tasks before spawning
    // Ground truth map: Key -> (task_id, item_idx, expected_value, tx_id)
    let mut ground_truth: HashMap<Vec<u8>, (usize, usize, Vec<u8>, TxId)> = HashMap::new();
    let mut task_items: Vec<Vec<(TxId, Vec<u8>, Vec<u8>)>> = vec![Vec::new(); num_tasks];

    for t in 0..num_tasks {
        for i in 0..items_per_task {
            let tx_id = TxId::new((t * 1000 + i + 1) as u64);
            let key = format!("chaos_k_t{:02}_i{:02}", t, i).into_bytes();
            let val = format!("chaos_v_t{:02}_i{:02}_payload_bytes", t, i).into_bytes();

            ground_truth.insert(key.clone(), (t, i, val.clone(), tx_id));
            task_items[t].push((tx_id, key, val));
        }
    }

    // 2. Spawn N tokio tasks executing sequential commits
    let mut handles = Vec::new();
    for t in 0..num_tasks {
        let st = Arc::clone(&storage);
        let items = task_items[t].clone();
        let handle = tokio::spawn(async move {
            let mut committed_keys = Vec::new();
            for (tx_id, key, val) in items {
                if st.put(tx_id, &key, &val).await.is_ok() {
                    if st.commit(tx_id).await.is_ok() {
                        committed_keys.push((key, val));
                    }
                }
                // Short sleep/yield to create concurrent execution overlap and abort race windows
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            committed_keys
        });
        handles.push(handle);
    }

    // 3. Short ramp-up time before aborting
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Randomly select ~30% (15) tasks to abort
    let mut rng = rand::thread_rng();
    let mut task_indices: Vec<usize> = (0..num_tasks).collect();
    task_indices.shuffle(&mut rng);

    let num_to_abort = (num_tasks as f64 * 0.30) as usize; // 15 tasks
    let aborted_indices: HashSet<usize> = task_indices[..num_to_abort].iter().copied().collect();

    for &idx in &aborted_indices {
        handles[idx].abort();
    }

    // 4. Await all handles, ignoring cancellation errors for aborted tasks
    let mut non_aborted_committed_keys: HashMap<Vec<u8>, Vec<u8>> = HashMap::new();

    for (t, handle) in handles.into_iter().enumerate() {
        match handle.await {
            Ok(committed_keys) => {
                for (k, v) in committed_keys {
                    non_aborted_committed_keys.insert(k, v);
                }
            }
            Err(join_err) => {
                if join_err.is_cancelled() {
                    // Task was aborted as expected
                } else {
                    panic!("Task {} failed with non-cancellation error: {:?}", t, join_err);
                }
            }
        }
    }

    // 5. Assertions:
    // a. For every key whose task was NOT aborted AND whose commit was reported successful:
    //    Value MUST be correctly readable via `get`.
    for (k, expected_v) in &non_aborted_committed_keys {
        let read_res = storage.get(k).await.expect("storage.get failed");
        assert_eq!(
            read_res,
            Some(expected_v.clone()),
            "Key {:?} from completed commit was missing or corrupted",
            String::from_utf8_lossy(k)
        );
    }

    // b. For keys from aborted tasks:
    //    Either fully visible (commit was completed before abort) or fully invisible (None).
    //    No value may be half-written or corrupt.
    for (key, (t, _i, expected_v, _tx_id)) in &ground_truth {
        if aborted_indices.contains(t) {
            let read_res = storage
                .get(key)
                .await
                .expect("storage.get failed for aborted task key");
            match read_res {
                Some(read_v) => {
                    assert_eq!(
                        read_v,
                        *expected_v,
                        "Key {:?} from aborted task {} was visible but corrupted/partial",
                        String::from_utf8_lossy(key),
                        t
                    );
                }
                None => {
                    // Fully invisible — valid uncommitted or aborted state
                }
            }
        }
    }

    // Record pre-reopen state for all keys in ground truth
    let mut pre_reopen_state: HashMap<Vec<u8>, Option<Vec<u8>>> = HashMap::new();
    for key in ground_truth.keys() {
        let val = storage
            .get(key)
            .await
            .expect("get failed during pre-reopen snapshot");
        pre_reopen_state.insert(key.clone(), val);
    }

    // Gracefully close storage and drop instance
    storage.close().await.expect("close storage failed");
    drop(storage);

    // c. Subsequent full LsmStorage reopen (Drop + open anew) must not panic
    //    and must yield exactly the same data state as before reopen (recovery determinism).
    let reopened_storage = LsmStorage::new(config)
        .await
        .expect("LsmStorage recovery reopen failed");

    for (key, expected_opt_val) in &pre_reopen_state {
        let read_val = reopened_storage
            .get(key)
            .await
            .expect("get failed post-reopen");
        assert_eq!(
            read_val,
            *expected_opt_val,
            "Post-reopen recovery state mismatch for key {:?}",
            String::from_utf8_lossy(key)
        );
    }
}
