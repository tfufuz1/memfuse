//! Cross-crate stress test for MemFuse DB and Checkpoint Manager.
// ANCHOR:INTEGRATION:STRESS-002 STATUS:READY AGENT:12 DATE:2026-05-18

use memfuse_checkpoint::CheckpointManager;
use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::task::JoinHandle;

#[tokio::test(flavor = "multi_thread")]
async fn test_db_checkpoint_interaction_stress() {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        max_elements: 10000,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };

    let db = Arc::new(
        MemFuse::open_with_config(tmp.path(), config)
            .await
            .expect("open db"),
    );
    let cp_manager = Arc::new(CheckpointManager::new(db.inner_storage()));

    let num_tasks = 15;
    let ops_per_task = 30;
    let mut handles: Vec<JoinHandle<()>> = Vec::new();

    for t in 0..num_tasks {
        let db = db.clone();
        let cp_manager = cp_manager.clone();
        handles.push(tokio::spawn(async move {
            let col_name = format!("col-{}", t);
            let col = db.collection(&col_name).await.expect("collection");

            for i in 0..ops_per_task {
                let id = format!("doc-{}", i);
                let vec = vec![i as f32, (i + 1) as f32, 0.0, 0.0];

                // 1. Insert
                col.insert(&id, &vec, Some(json!({"t": t, "i": i})))
                    .await
                    .expect("insert");

                // 2. Search
                let results = col.search(&vec, 1).await.expect("search");
                assert!(!results.is_empty());

                // 3. Random Checkpoint operation
                if i % 10 == 0 {
                    let cp_name = format!("cp-{}-{}", t, i);
                    let cp = cp_manager
                        .create_checkpoint(&cp_name)
                        .await
                        .expect("create cp");
                    // Just drop it immediately to simulate load
                    cp_manager.drop_checkpoint(&cp).await.expect("drop cp");
                }

                // 4. Delete some
                if i % 5 == 0 {
                    col.delete(&id).await.expect("delete");
                }
            }
        }));
    }

    for h in handles {
        h.await.expect("task failed");
    }

    // Verify consistency
    let collections = db.list_collections().await.expect("list collections");
    assert!(collections.len() >= num_tasks);
}
