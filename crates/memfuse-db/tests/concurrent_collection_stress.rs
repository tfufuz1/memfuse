//! High-concurrency stress tests for a single MemFuse collection.
// ANCHOR:INTEGRATION:STRESS-001 STATUS:DONE AGENT:12 DATE:2026-05-18

use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::task::JoinHandle;

#[tokio::test(flavor = "multi_thread")]
async fn test_concurrent_collection_ops() {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        max_elements: 10000,
        distance_metric: DistanceMetric::Cosine,
        encryption_passphrase: None,
    };
    let db = Arc::new(
        MemFuse::open_with_config(tmp.path(), config)
            .await
            .expect("open db"),
    );

    let col = Arc::new(db.collection("shared-stress").await.expect("collection"));

    let num_tasks = 20;
    let ops_per_task = 50;
    let mut handles: Vec<JoinHandle<()>> = Vec::new();

    for t in 0..num_tasks {
        let col = col.clone();
        handles.push(tokio::spawn(async move {
            for i in 0..ops_per_task {
                let id = format!("task-{}-doc-{}", t, i);
                let vec = vec![t as f32, i as f32, 0.0, 0.0];

                // Insert
                col.insert(&id, &vec, Some(json!({"t": t, "i": i})))
                    .await
                    .expect("insert");

                // Search
                let results = col.search(&vec, 1).await.expect("search");
                assert!(
                    !results.is_empty(),
                    "Search should find at least one result (itself)"
                );

                // Delete
                col.delete(&id).await.expect("delete");
            }
        }));
    }

    for h in handles {
        h.await.expect("task panicked");
    }

    // Final sanity check: collection should be empty
    let final_len = col.len().await;
    assert_eq!(
        final_len, 0,
        "Collection should be empty after all deletes, but has {} docs",
        final_len
    );
}
