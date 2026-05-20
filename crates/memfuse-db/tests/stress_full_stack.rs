//! Full-stack stress tests for MemFuse.
// ANCHOR:INTEGRATION:STRESS-002 STATUS:READY AGENT:12 DATE:2026-05-20

use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::task::JoinHandle;

#[tokio::test(flavor = "multi_thread")]
async fn test_full_stack_concurrency_stress() {
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
            .expect("open db"),
    );

    let num_tasks = 20;
    let ops_per_task = 30;
    let mut handles: Vec<JoinHandle<()>> = Vec::new();

    for t in 0..num_tasks {
        let db = db.clone();
        handles.push(tokio::spawn(async move {
            let col_name = format!("stress-col-{}", t);
            let col = db.collection(&col_name).await.expect("collection");

            for i in 0..ops_per_task {
                let id = format!("task-{}-doc-{}", t, i);
                let vec = vec![t as f32, i as f32, (t+i) as f32, 0.0];
                let text = format!("This is content for document {} in task {}", i, t);

                // 1. Insert
                col.insert(&id, &vec, Some(json!({"t": t, "i": i, "text": text})))
                    .await
                    .expect("insert");

                // 2. Hybrid Search - should find itself by vector and text
                let results = col.hybrid_search(&text, &vec, 5).await.expect("hybrid search");
                assert!(!results.is_empty(), "Task {} doc {} search results empty", t, i);

                let found = results.iter().any(|r| r.id == id);
                assert!(found, "Task {} doc {} not found in its own hybrid search results", t, i);

                // 3. Delete
                col.delete(&id).await.expect("delete");

                // 4. Verify gone
                let doc = col.get(&id).await.expect("get");
                assert!(doc.is_none(), "Task {} doc {} still exists after delete", t, i);
            }
        }));
    }

    for h in handles {
        h.await.expect("task panicked");
    }

    // Final sanity check: all collections should be empty
    for t in 0..num_tasks {
        let col_name = format!("stress-col-{}", t);
        let col = db.collection(&col_name).await.expect("collection");
        assert_eq!(col.len().await, 0, "Collection {} not empty", col_name);
    }
}
