//! Multi-threaded Stress Tests for MemFuse.
//!
//! ANCHOR:TEST:STRESS-CONCURRENCY-001 STATUS:READY AGENT:12

use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_high_concurrency_insert_search_delete() {
    let tmp = TempDir::new().expect("Failed to create temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        max_elements: 10000,
        distance_metric: DistanceMetric::Cosine,
    };
    let db = Arc::new(MemFuse::open_with_config(tmp.path(), config).await.expect("Failed to open DB"));

    let num_tasks = 8;
    let ops_per_task = 50;
    let mut handles = Vec::new();

    for t in 0..num_tasks {
        let db = db.clone();
        handles.push(tokio::spawn(async move {
            let col = db.collection(&format!("stress-{}", t)).await.unwrap();
            for i in 0..ops_per_task {
                let id = format!("doc-{}", i);
                let vec = vec![i as f32, (i + 1) as f32, (i + 2) as f32, (i + 3) as f32];

                // Concurrent Ops
                col.insert(&id, &vec, Some(json!({"t": t, "i": i}))).await.unwrap();
                let res = col.search(&vec, 5).await.unwrap();
                assert!(res.iter().any(|r| r.id == id), "Expected id {} in results {:?}", id, res);

                if i % 2 == 0 {
                    col.delete(&id).await.unwrap();
                    assert!(col.get(&id).await.unwrap().is_none());
                }
            }
        }));
    }

    for h in handles {
        h.await.expect("Task panicked");
    }
}
