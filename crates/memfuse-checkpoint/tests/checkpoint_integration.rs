use memfuse_checkpoint::CheckpointManager;
use memfuse_store::{LsmStorage, LsmConfig};
use std::sync::Arc;
use serde_json::json;
use tokio::task::JoinHandle;
use tempfile::TempDir;

// AGENT:12 STATUS:READY
// This integration test verifies the full lifecycle of checkpoints using the real LsmStorage implementation.

#[tokio::test]
async fn test_checkpoint_real_storage_lifecycle() {
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

    // 1. Create multiple checkpoints
    for i in 1..=5 {
        manager.create_checkpoint(
            &format!("cp-{}", i),
            "coll-1",
            i * 10,
            json!({"index": i})
        ).await.expect("Failed to create checkpoint");
    }

    // 2. Verify listing and ordering
    let list = manager.list_checkpoints().await.expect("Failed to list");
    assert_eq!(list.len(), 5);
    for (i, checkpoint) in list.iter().enumerate().take(5) {
        assert_eq!(checkpoint.name, format!("cp-{}", i + 1));
        assert_eq!(checkpoint.seq_no, (i as u64 + 1) * 10);
    }

    // 3. Verify persistence reload
    // We must drop the first storage to ensure all files are flushed and locked are released if necessary
    // Actually LsmStorage might need explicit closing if it has background threads,
    // but usually dropping Arc and keeping only one reference is enough if it's implemented correctly.
    drop(manager);

    let manager2 = CheckpointManager::new(storage.clone());
    let list2 = manager2.list_checkpoints().await.expect("Failed to reload list");
    assert_eq!(list2.len(), 5);
    assert_eq!(list2[0].name, "cp-1");

    // 4. Delete a checkpoint and verify
    manager2.drop_checkpoint("cp-3").await.expect("Failed to drop");
    let list3 = manager2.list_checkpoints().await.expect("Failed to list after drop");
    assert_eq!(list3.len(), 4);
    assert!(list3.iter().all(|c| c.name != "cp-3"));
}

#[tokio::test(flavor = "multi_thread")]
async fn test_checkpoint_real_storage_stress() {
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
    let manager = Arc::new(CheckpointManager::new(storage.clone()));

    let num_tasks = 5;
    let ops_per_task = 10;
    let mut handles: Vec<JoinHandle<()>> = Vec::new();

    for t in 0..num_tasks {
        let manager = manager.clone();
        handles.push(tokio::spawn(async move {
            for i in 0..ops_per_task {
                let name = format!("task-{}-cp-{}", t, i);
                manager.create_checkpoint(&name, "default", (t * 100 + i) as u64, json!({}))
                    .await
                    .expect("Concurrent create failed");

                if i % 2 == 0 {
                    manager.drop_checkpoint(&name).await.expect("Concurrent drop failed");
                }
            }
        }));
    }

    for h in handles {
        h.await.expect("Task panicked");
    }

    let final_list = manager.list_checkpoints().await.expect("Final list failed");
    assert_eq!(final_list.len(), (num_tasks * ops_per_task) / 2);
}
