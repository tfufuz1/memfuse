//! Multi-threaded Stress Tests for MemFuse.
//!
//! ANCHOR:TEST:STRESS-CONCURRENCY-001 STATUS:READY AGENT:12

use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test(flavor = "multi_thread")]
async fn test_high_concurrency_lifecycle() {
    let tmp = TempDir::new().expect("Failed to create temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        max_elements: 10000,
        distance_metric: DistanceMetric::Cosine,
    };
    let db = Arc::new(MemFuse::open_with_config(tmp.path(), config).await.expect("Failed to open DB"));
    let col = Arc::new(db.collection("stress-test").await.unwrap());

    let num_tasks = 10;
    let ops_per_task = 20;
    let mut handles = Vec::new();

    for t in 0..num_tasks {
        let col = col.clone();
        handles.push(tokio::spawn(async move {
            for i in 0..ops_per_task {
                let id = format!("task-{}-doc-{}", t, i);
                let vec = vec![i as f32, 0.0, 0.0, 0.0];

                // 1. Insert
                col.insert(&id, &vec, Some(json!({"t": t, "i": i}))).await.unwrap();

                // 2. Search
                let res = col.search(&vec, 5).await.unwrap();
                assert!(res.iter().any(|r| r.id == id));

                // 3. Delete
                col.delete(&id).await.unwrap();

                // 4. Verify gone
                assert!(col.get(&id).await.unwrap().is_none());
            }
        }));
    }

    for h in handles {
        h.await.expect("Task panicked");
    }

    // 5. Final Consistency Check
    assert_eq!(col.len().await, 0, "Collection should be empty after all deletions");
}
