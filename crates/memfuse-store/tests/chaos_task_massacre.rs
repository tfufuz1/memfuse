//! Chaos test for concurrent task cancellation (massacre) during LSM storage writes.

use memfuse_core::{StorageEngine, TxId};
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

#[derive(Debug, Clone)]
struct PlannedItem {
    task_id: usize,
    key: Vec<u8>,
    value: Vec<u8>,
    tx_id: TxId,
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_chaos_task_massacre() {
    let temp_dir = TempDir::new().expect("temp dir");
    let config = LsmConfig {
        path: temp_dir.path().to_path_buf(),
        memtable_size_limit: 64 * 1024, // Small 64KB memtable to force frequent flush / WAL interaction
        ..Default::default()
    };

    let storage = Arc::new(LsmStorage::new(config.clone()).await.expect("create storage"));

    const NUM_TASKS: usize = 50;
    const OPS_PER_TASK: usize = 30;

    // 1. Pre-generate ground truth for all planned (Key, Value) pairs outside tasks
    let mut planned_items: Vec<PlannedItem> = Vec::with_capacity(NUM_TASKS * OPS_PER_TASK);
    let mut tx_counter = 1u64;

    for task_id in 0..NUM_TASKS {
        for op_idx in 0..OPS_PER_TASK {
            let key = format!("task-{:02}-key-{:04}", task_id, op_idx).into_bytes();
            let value = format!("val-payload-{:02}-{:04}-data", task_id, op_idx).into_bytes();
            let tx_id = TxId::new(tx_counter);
            tx_counter += 1;

            planned_items.push(PlannedItem {
                task_id,
                key,
                value,
                tx_id,
            });
        }
    }

    // Shared thread-safe recording of keys whose commit returned Ok(())
    let committed_keys = Arc::new(parking_lot::Mutex::new(HashSet::new()));

    // 2. Spawn N tasks
    let mut handles = Vec::with_capacity(NUM_TASKS);

    for task_id in 0..NUM_TASKS {
        let storage_clone = Arc::clone(&storage);
        let committed_keys_clone = Arc::clone(&committed_keys);
        let task_items: Vec<PlannedItem> = planned_items
            .iter()
            .filter(|item| item.task_id == task_id)
            .cloned()
            .collect();

        let handle = tokio::spawn(async move {
            for item in task_items {
                storage_clone
                    .put(item.tx_id, &item.key, &item.value)
                    .await
                    .expect("put operation should succeed");

                storage_clone
                    .commit(item.tx_id)
                    .await
                    .expect("commit operation should succeed");

                committed_keys_clone.lock().insert(item.key.clone());
            }
        });

        handles.push(handle);
    }

    // 3. Chaos injection: abort a random ~30% subset of tasks during execution
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Select 30% (15 tasks out of 50) to abort
    let mut abort_indices: Vec<usize> = (0..NUM_TASKS).collect();
    use rand::seq::SliceRandom;
    let mut rng = rand::thread_rng();
    abort_indices.shuffle(&mut rng);
    let aborted_set: HashSet<usize> = abort_indices.into_iter().take(15).collect();

    for &aborted_idx in &aborted_set {
        handles[aborted_idx].abort();
    }

    // 4. Await all handles, ignoring cancellations, failing on unexpected errors
    for (task_id, handle) in handles.into_iter().enumerate() {
        match handle.await {
            Ok(_) => {
                // Task completed without being aborted or before abort took effect
            }
            Err(err) => {
                if err.is_cancelled() {
                    assert!(
                        aborted_set.contains(&task_id),
                        "Task {} was cancelled but was not in aborted_set",
                        task_id
                    );
                } else if err.is_panic() {
                    std::panic::resume_unwind(err.into_panic());
                } else {
                    panic!("Task {} failed with unexpected error: {:?}", task_id, err);
                }
            }
        }
    }

    let snapshot_committed = committed_keys.lock().clone();

    // 5a. Non-aborted completed commits MUST be readable
    for item in &planned_items {
        if !aborted_set.contains(&item.task_id) && snapshot_committed.contains(&item.key) {
            let res = storage
                .get(&item.key)
                .await
                .expect("get key from non-aborted task");
            assert_eq!(
                res,
                Some(item.value.clone()),
                "Key {:?} from non-aborted task {} must be correctly readable",
                String::from_utf8_lossy(&item.key),
                item.task_id
            );
        }
    }

    // 5b. Aborted task keys MUST be fully visible OR fully invisible (no partial or corrupt state)
    let mut pre_reopen_state: Vec<(Vec<u8>, Option<Vec<u8>>)> = Vec::with_capacity(planned_items.len());

    for item in &planned_items {
        let val = storage
            .get(&item.key)
            .await
            .expect("get key pre-reopen");

        if aborted_set.contains(&item.task_id) {
            match &val {
                Some(v) => {
                    assert_eq!(
                        v, &item.value,
                        "Key {:?} from aborted task {} was visible, but value was corrupted",
                        String::from_utf8_lossy(&item.key),
                        item.task_id
                    );
                }
                None => {
                    // Fully invisible — valid atomic outcome for cancelled task
                }
            }
        }

        pre_reopen_state.push((item.key.clone(), val));
    }

    // 5c. Storage reopen (Drop + reopen) MUST NOT panic and MUST yield identical state (recovery determinism)
    drop(storage);

    let storage_reopened = LsmStorage::new(config)
        .await
        .expect("reopen storage after massacre");

    for (key, expected_pre_val) in pre_reopen_state {
        let reopened_val = storage_reopened
            .get(&key)
            .await
            .expect("get key post-reopen");

        assert_eq!(
            reopened_val, expected_pre_val,
            "State mismatch after reopen for key {:?} (Recovery determinism violated)",
            String::from_utf8_lossy(&key)
        );
    }
}
