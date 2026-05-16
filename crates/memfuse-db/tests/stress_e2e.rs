//! Stress tests for MemFuse DB.
//! Tests high concurrency and stability under load.

use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::task::JoinHandle;

#[tokio::test(flavor = "multi_thread")]
async fn test_high_concurrency_stress() {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 8,
        max_elements: 5000,
        distance_metric: DistanceMetric::Cosine,
    };
    let db = Arc::new(
        MemFuse::open_with_config(tmp.path(), config)
            .await
            .expect("open db"),
    );

    let num_collections = 5;
    let tasks_per_col = 4;
    let ops_per_task = 100;
    let mut handles: Vec<JoinHandle<()>> = Vec::new();

    for c in 0..num_collections {
        let col_name = format!("stress-col-{}", c);
        for t in 0..tasks_per_col {
            let db = db.clone();
            let col_name = col_name.clone();
            handles.push(tokio::spawn(async move {
                let col = db.collection(&col_name).await.expect("collection");
                for i in 0..ops_per_task {
                    let id = format!("task-{}-doc-{}", t, i);
                    let vec = vec![i as f32; 8];

                    // Interleaved CRUD
                    col.insert(&id, &vec, Some(json!({"val": i})))
                        .await
                        .expect("insert");

                    if i % 2 == 0 {
                        let _ = col.search(&vec, 5).await.expect("search");
                    }

                    if i % 5 == 0 {
                        col.delete(&id).await.expect("delete");
                    }
                }
            }));
        }
    }

    for h in handles {
        h.await.expect("task failed");
    }

    // Final consistency check
    for c in 0..num_collections {
        let col_name = format!("stress-col-{}", c);
        let col = db.collection(&col_name).await.expect("col");
        let len = col.len().await;
        // Expected len: ops_per_task * tasks_per_col - (deleted docs)
        // deleted if i % 5 == 0 -> 20 docs per task. 100 - 20 = 80 docs per task.
        // 4 tasks * 80 = 320 docs.
        assert_eq!(len, 320, "Collection {} has unexpected size", col_name);
    }
}
