// FILE-CONTEXT: Chaos task massacre test for concurrent writer aborts and recovery determinism.
//! Chaos test verifying resilience against concurrent task aborts, no partial state corruption,
//! and deterministic recovery after restart.

use memfuse_core::{StorageEngine, TxId};
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use parking_lot::Mutex;
use rand::seq::SliceRandom;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_chaos_task_massacre() {
    let temp_dir = TempDir::new().expect("temp dir");
    let config = LsmConfig {
        path: temp_dir.path().to_path_buf(),
        memtable_size_limit: 64 * 1024, // 64 KB to trigger flushes under load
        ..Default::default()
    };

    let storage = Arc::new(LsmStorage::new(config.clone()).await.expect("storage init"));

    let num_tasks = 50;
    let keys_per_task = 20;

    #[derive(Clone, Debug)]
    struct TaskPlan {
        task_id: usize,
        items: Vec<(Vec<u8>, Vec<u8>)>,
    }

    let mut ground_truth_plan = Vec::with_capacity(num_tasks);
    let mut all_ground_truth_keys = HashMap::new();

    for task_id in 0..num_tasks {
        let mut items = Vec::with_capacity(keys_per_task);
        for k_idx in 0..keys_per_task {
            let key = format!("task_{:02}_key_{:03}", task_id, k_idx).into_bytes();
            let value = format!(
                "val_task_{:02}_idx_{:03}_payload_data_padding_bytes_x",
                task_id, k_idx
            )
            .into_bytes();
            all_ground_truth_keys.insert(key.clone(), (task_id, value.clone()));
            items.push((key, value));
        }
        ground_truth_plan.push(TaskPlan { task_id, items });
    }

    let confirmed_commits: Arc<Mutex<HashSet<Vec<u8>>>> = Arc::new(Mutex::new(HashSet::new()));

    let mut handles = Vec::with_capacity(num_tasks);

    for plan in ground_truth_plan {
        let storage = Arc::clone(&storage);
        let confirmed_commits = Arc::clone(&confirmed_commits);
        let task_id = plan.task_id;

        let handle = tokio::spawn(async move {
            for (step, (key, value)) in plan.items.into_iter().enumerate() {
                let tx_id = TxId::new(((task_id + 1) * 100_000 + step + 1) as u64);

                storage.put(tx_id, &key, &value).await.expect("put");
                storage.commit(tx_id).await.expect("commit");

                confirmed_commits.lock().insert(key);

                // Small delay between commits to spread task execution over time and allow aborts to hit mid-flight
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        });

        handles.push((task_id, handle));
    }

    // Short warmup phase before aborting ~30% of tasks mid-execution
    tokio::time::sleep(Duration::from_millis(8)).await;

    let num_aborts = (num_tasks as f64 * 0.30) as usize; // 15
    let mut rng = rand::thread_rng();
    let mut task_indices: Vec<usize> = (0..num_tasks).collect();
    task_indices.shuffle(&mut rng);

    let aborted_task_ids: HashSet<usize> = task_indices.into_iter().take(num_aborts).collect();

    for (task_id, handle) in &handles {
        if aborted_task_ids.contains(task_id) {
            handle.abort();
        }
    }

    // Await all remaining and aborted task handles
    for (task_id, handle) in handles {
        match handle.await {
            Ok(()) => {
                // Task finished normally (or completed before abort took effect)
            }
            Err(err) => {
                if err.is_cancelled() {
                    assert!(
                        aborted_task_ids.contains(&task_id),
                        "Task {} was cancelled but was not in aborted_task_ids set",
                        task_id
                    );
                } else {
                    panic!(
                        "Task {} failed with unexpected join error: {:?}",
                        task_id, err
                    );
                }
            }
        }
    }

    let confirmed = confirmed_commits.lock().clone();

    // 5a & 5b Assertions: Check every key in ground truth
    let mut pre_reopen_snapshot: HashMap<Vec<u8>, Option<Vec<u8>>> = HashMap::new();

    for (key, (task_id, expected_val)) in &all_ground_truth_keys {
        let actual_val = storage.get(key).await.expect("get failed");

        pre_reopen_snapshot.insert(key.clone(), actual_val.clone());

        let is_aborted_task = aborted_task_ids.contains(task_id);
        let commit_confirmed = confirmed.contains(key);

        if !is_aborted_task {
            // 5a. Non-aborted task: key MUST have been confirmed committed AND be correctly readable
            assert!(
                commit_confirmed,
                "Key {:?} from non-aborted task {} missing in confirmed commits",
                String::from_utf8_lossy(key),
                task_id
            );
            assert_eq!(
                actual_val,
                Some(expected_val.clone()),
                "Key {:?} from non-aborted task {} returned incorrect or missing value",
                String::from_utf8_lossy(key),
                task_id
            );
        } else {
            // 5b. Aborted task:
            if commit_confirmed {
                // Commit returned Ok(()) before abort -> MUST be correctly readable
                assert_eq!(
                    actual_val,
                    Some(expected_val.clone()),
                    "Confirmed key {:?} from aborted task {} returned incorrect value",
                    String::from_utf8_lossy(key),
                    task_id
                );
            } else {
                // Commit did not complete before abort -> MUST be either fully visible or fully invisible (None), never corrupt
                if let Some(ref val) = actual_val {
                    assert_eq!(
                        val,
                        expected_val,
                        "Key {:?} from aborted task {} has corrupted/partial value",
                        String::from_utf8_lossy(key),
                        task_id
                    );
                }
            }
        }
    }

    // 5c. Recovery Determinism: Gracefully close and reopen LsmStorage
    storage.close().await.expect("storage close");
    drop(storage);

    let storage_reopened = LsmStorage::new(config).await.expect("reopen storage");

    for (key, expected_opt_val) in &pre_reopen_snapshot {
        let reopened_val = storage_reopened.get(key).await.expect("reopened get");
        assert_eq!(
            &reopened_val,
            expected_opt_val,
            "Recovery determinism mismatch for key {:?}: pre-reopen {:?}, post-reopen {:?}",
            String::from_utf8_lossy(key),
            expected_opt_val,
            reopened_val
        );
    }
}
