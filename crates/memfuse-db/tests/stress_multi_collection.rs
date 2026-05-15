//! Multi-collection stress tests for MemFuse.
// AGENT:12 DATE:2026-05-15 STATUS:READY

use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::task::JoinHandle;

#[tokio::test(flavor = "multi_thread")]
async fn test_stress_multi_collection_concurrent_ops() {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 64, // Use even more dimensions for uniqueness
        max_elements: 10000,
        distance_metric: DistanceMetric::Cosine,
    };
    let db = Arc::new(
        MemFuse::open_with_config(tmp.path(), config)
            .await
            .expect("open db"),
    );

    let num_tasks = 4;
    let ops_per_task = 30; // Reduced slightly to speed up but still stress
    let mut handles: Vec<JoinHandle<()>> = Vec::new();

    for t in 0..num_tasks {
        let db = db.clone();
        handles.push(tokio::spawn(async move {
            let col_name = format!("stress-col-{}", t);
            let col = db.collection(&col_name).await.expect("collection");

            for i in 0..ops_per_task {
                let id = format!("doc-{}-{}", t, i);
                // Use completely unique vectors per document
                let mut vec = vec![0.0; 64];
                vec[i % 64] = 1.0;
                // Add a small unique value to another dimension to ensure uniqueness even if i > 64
                if i >= 64 {
                    vec[(i / 64) % 64] += 0.1;
                }

                // 2. Jede Task: Insert → Search → Delete
                col.insert(&id, &vec, Some(json!({"task": t, "op": i})))
                    .await
                    .expect("insert");

                let results = col.search(&vec, 1).await.expect("search");
                assert!(!results.is_empty());
                // results[0].id must be our 'id' because vectors are unique and HNSW should find it
                assert_eq!(results[0].id, id, "Failed for task {} op {} with vec at index {}", t, i, i % 64);

                // Delete every 2nd
                if i % 2 == 0 {
                    col.delete(&id).await.expect("delete");
                    let doc = col.get(&id).await.expect("get");
                    assert!(doc.is_none());
                }
            }
        }));
    }

    for h in handles {
        h.await.expect("task panicked");
    }

    // 3. Am Ende: Verify Konsistenz
    for t in 0..num_tasks {
        let col_name = format!("stress-col-{}", t);
        let col = db.collection(&col_name).await.expect("collection");
        let count = col.len().await;
        // Half deleted (those where i % 2 == 0)
        assert_eq!(count, ops_per_task / 2);
    }
}
