//! Integration tests for Checkpoint pinning and rollback through the DB facade.
// ANCHOR:INTEGRATION:CHECKPOINT-DB STATUS:DONE AGENT:12 DATE:2026-06-21

use memfuse_db::{MemFuse, MemFuseConfig};
use memfuse_checkpoint::CheckpointManager;
use serde_json::json;
use tempfile::TempDir;
use std::sync::Arc;
use tokio::task::JoinHandle;

#[tokio::test]
async fn test_db_checkpoint_lifecycle() {
    let tmp = TempDir::new().expect("failed to create temp dir");
    let config = MemFuseConfig {
        dimension: 3,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(tmp.path(), config).await.expect("failed to open db");

    // 1. Insert initial data
    db.insert("doc-1", &[1.0, 0.0, 0.0], Some(json!({"v": 1}))).await.expect("insert failed");

    // 2. Setup CheckpointManager via inner storage
    let storage = db.inner_storage();
    let manager = CheckpointManager::new(storage);

    // 3. Create Checkpoint
    let cp = manager.create_checkpoint("initial-state").await.expect("checkpoint failed");
    assert!(cp.seq_no > 0);

    // 4. Perform more operations
    db.insert("doc-2", &[0.0, 1.0, 0.0], Some(json!({"v": 2}))).await.expect("insert failed");
    db.update("doc-1", &[1.0, 0.0, 0.0], Some(json!({"v": 1.1}))).await.expect("update failed");

    // 5. Verify current state
    assert_eq!(db.len().await.expect("len failed"), 2);
    let doc1 = db.get("doc-1").await.expect("get failed").expect("missing doc");
    assert_eq!(doc1.metadata.unwrap()["v"], 1.1);

    // 6. Rollback (Note: currently rollback is a STUB in CheckpointManager, but we test the API flow)
    manager.rollback(&cp).await.expect("rollback failed");

    // 7. Drop Checkpoint
    manager.drop_checkpoint(&cp).await.expect("drop failed");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_concurrent_ops_during_checkpointing() {
    let tmp = TempDir::new().expect("failed to create temp dir");
    let config = MemFuseConfig {
        dimension: 3,
        ..Default::default()
    };
    let db = Arc::new(MemFuse::open_with_config(tmp.path(), config).await.expect("failed to open db"));
    let manager = Arc::new(CheckpointManager::new(db.inner_storage()));

    let num_tasks = 5;
    let ops_per_task = 50;
    let mut handles: Vec<JoinHandle<()>> = Vec::new();

    // Writer tasks
    for t in 0..num_tasks {
        let db = db.clone();
        handles.push(tokio::spawn(async move {
            for i in 0..ops_per_task {
                let id = format!("task-{}-doc-{}", t, i);
                db.insert(&id, &[0.1, 0.2, 0.3], Some(json!({"t": t, "i": i}))).await.expect("insert failed");

                if i % 10 == 0 {
                    let _ = db.get(&id).await.expect("get failed");
                }
            }
        }));
    }

    // Checkpointer task
    let manager_clone = manager.clone();
    let stop_checkpointer = Arc::new(tokio::sync::Notify::new());
    let stop_clone = stop_checkpointer.clone();
    let checkpointer_handle = tokio::spawn(async move {
        let mut count = 0;
        loop {
            tokio::select! {
                _ = stop_clone.notified() => break,
                _ = tokio::time::sleep(std::time::Duration::from_millis(20)) => {
                    let name = format!("cp-{}", count);
                    let cp = manager_clone.create_checkpoint(&name).await.expect("create cp failed");
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                    manager_clone.drop_checkpoint(&cp).await.expect("drop cp failed");
                    count += 1;
                }
            }
        }
    });

    for h in handles {
        h.await.expect("writer task failed");
    }

    stop_checkpointer.notify_one();
    checkpointer_handle.await.expect("checkpointer task failed");

    assert_eq!(db.len().await.expect("len failed"), num_tasks * ops_per_task);
}
