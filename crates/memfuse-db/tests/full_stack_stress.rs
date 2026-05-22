//! Workspace-wide stress test involving DB, Index, Store, and Checkpoint.
// ANCHOR:INTEGRATION:FULL-STACK-STRESS STATUS:READY AGENT:12 DATE:2026-05-22

use memfuse_checkpoint::CheckpointManager;
use memfuse_db::{MemFuse, MemFuseConfig};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::task::JoinHandle;

#[tokio::test(flavor = "multi_thread")]
async fn test_full_stack_concurrency_with_checkpoints() {
    let tmp = TempDir::new().expect("temp dir");
    let db_path = tmp.path().to_path_buf();

    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };

    let db = Arc::new(
        MemFuse::open_with_config(&db_path, config)
            .await
            .expect("open db"),
    );
    let storage = db.inner_storage();
    let cp_manager = Arc::new(CheckpointManager::new(storage));

    let num_tasks = 10;
    let ops_per_task = 30;
    let mut handles: Vec<JoinHandle<()>> = Vec::new();

    // 1. Concurrent Mutation Tasks
    for t in 0..num_tasks {
        let db = db.clone();
        handles.push(tokio::spawn(async move {
            let col = db.collection(&format!("col-{}", t)).await.unwrap();
            for i in 0..ops_per_task {
                let id = format!("doc-{}", i);
                let vec = vec![1.0, (t as f32) / 10.0, (i as f32) / 100.0, 0.0];

                col.insert(&id, &vec, Some(json!({"task": t, "idx": i})))
                    .await
                    .unwrap();

                if i % 10 == 0 {
                    // Periodic Hybrid Search
                    let _ = col.hybrid_search("task", &vec, 5).await.unwrap();
                }
            }
        }));
    }

    // 2. Concurrent Checkpoint Task
    let cp_manager_clone = cp_manager.clone();
    let cp_handle = tokio::spawn(async move {
        for i in 0..5 {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            cp_manager_clone
                .create_checkpoint(
                    &format!("cp-{}", i),
                    "col-0",
                    i * 10,
                    json!({"stress": true}),
                )
                .await
                .unwrap();
        }
    });

    for h in handles {
        h.await.expect("worker task failed");
    }
    cp_handle.await.expect("checkpoint task failed");

    // 3. Verification
    let cols = db.list_collections().await.unwrap();
    assert!(cols.len() >= num_tasks);

    let checkpoints = cp_manager.list_checkpoints().await.unwrap();
    assert_eq!(checkpoints.len(), 5);

    // 4. Persistence Reload Check
    drop(db);
    drop(cp_manager);

    let db2 = MemFuse::open_with_config(
        &db_path,
        MemFuseConfig {
            dimension: 4,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let cp_manager2 = CheckpointManager::new(db2.inner_storage());

    let reloaded_cols = db2.list_collections().await.unwrap();
    assert!(reloaded_cols.len() >= num_tasks);

    let reloaded_cp = cp_manager2.list_checkpoints().await.unwrap();
    assert_eq!(reloaded_cp.len(), 5);
}
