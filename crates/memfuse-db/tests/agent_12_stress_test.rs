//! Multi-collection high-concurrency stress tests for MemFuse.
// ANCHOR:INTEGRATION:STRESS-002 STATUS:READY AGENT:12 DATE:2026-05-21

use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::task::JoinHandle;

#[tokio::test(flavor = "multi_thread")]
async fn test_agent_12_concurrent_multi_collection_stress() {
    let tmp = TempDir::new().expect("Failed to create temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        max_elements: 10000,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };
    let db = Arc::new(
        MemFuse::open_with_config(tmp.path(), config)
            .await
            .expect("Failed to open DB"),
    );

    let num_tasks = 40;
    let ops_per_task = 100;
    let mut handles: Vec<JoinHandle<()>> = Vec::new();

    for t in 0..num_tasks {
        let db = db.clone();
        handles.push(tokio::spawn(async move {
            // Tasks work on different collections to stress the orchestrator and shared LSM
            let col_name = format!("stress-col-{}", t % 5);
            let col = db.collection(&col_name).await.expect("Failed to get collection");

            for i in 0..ops_per_task {
                let id = format!("task-{}-doc-{}", t, i);
                let vec = vec![t as f32, i as f32, (t + i) as f32, 0.0];

                // 1. Insert
                col.insert(&id, &vec, Some(json!({"t": t, "i": i})))
                    .await
                    .expect("Insert failed");

                // 2. Search (high probability of finding itself if k=10)
                let results = col.search(&vec, 10).await.expect("Search failed");
                assert!(!results.is_empty());
                assert!(results.iter().any(|r| r.id == id));

                // 3. Delete
                col.delete(&id).await.expect("Delete failed");

                // 4. Verify Gone
                let doc = col.get(&id).await.expect("Get failed");
                assert!(doc.is_none());
            }
        }));
    }

    for h in handles {
        h.await.expect("Task panicked");
    }

    // Final Consistency Check: all stress collections should be empty
    for t in 0..5 {
        let col_name = format!("stress-col-{}", t);
        let col = db.collection(&col_name).await.expect("Failed to get collection");
        assert_eq!(col.len().await, 0, "Collection {} is not empty", col_name);
    }
}
