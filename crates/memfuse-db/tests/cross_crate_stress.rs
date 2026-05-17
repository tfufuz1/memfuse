//! Cross-crate stress test involving memfuse-db and memfuse-checkpoint.
// ANCHOR:INTEGRATION:STRESS-002 STATUS:DONE AGENT:12 DATE:2026-05-18

use memfuse_checkpoint::CheckpointManager;
use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::time::{sleep, Duration};

#[tokio::test(flavor = "multi_thread")]
async fn test_cross_crate_db_and_checkpoint_stress() {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        max_elements: 1000,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };
    let db = Arc::new(
        MemFuse::open_with_config(tmp.path(), config)
            .await
            .expect("open db"),
    );

    let cp_manager = Arc::new(CheckpointManager::new(db.inner_storage()));

    let num_worker_tasks = 10;
    let ops_per_worker = 100;
    let mut handles = Vec::new();

    // Spawn CRUD workers
    for t in 0..num_worker_tasks {
        let db = db.clone();
        handles.push(tokio::spawn(async move {
            for i in 0..ops_per_worker {
                let id = format!("task-{}-doc-{}", t, i);
                let vec = vec![t as f32, i as f32, 0.0, 0.0];

                // Insert
                db.insert(&id, &vec, Some(json!({"t": t, "i": i})))
                    .await
                    .expect("insert");

                // Search
                if i % 10 == 0 {
                    let _results = db.search(&vec, 1).await.expect("search");
                }

                // Delete every 5th op to keep some data in DB
                if i % 5 == 4 {
                    db.delete(&id).await.expect("delete");
                }

                // Small sleep to allow interleaving
                sleep(Duration::from_millis(1)).await;
            }
        }));
    }

    // Spawn Checkpoint worker
    let cp_worker = {
        let cp_manager = cp_manager.clone();
        tokio::spawn(async move {
            for i in 0..5 {
                sleep(Duration::from_millis(20)).await;
                let cp_name = format!("cp-{}", i);
                let cp = cp_manager
                    .create_checkpoint(&cp_name)
                    .await
                    .expect("create cp");

                sleep(Duration::from_millis(10)).await;
                cp_manager.drop_checkpoint(&cp).await.expect("drop cp");
            }
        })
    };

    // Wait for all workers
    for h in handles {
        h.await.expect("worker task failed");
    }
    cp_worker.await.expect("checkpoint worker failed");

    // Final check
    let len = db.len().await.expect("db len");
    assert!(len > 0, "DB should not be empty after stress test");

    let stats = db.stats().await.expect("stats");
    assert!(stats.storage_stats.memtable_size_bytes > 0 || stats.storage_stats.num_segments > 0);
}
