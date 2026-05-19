//! Multi-collection stress tests for MemFuse.
// ANCHOR:INTEGRATION:STRESS-002 STATUS:READY AGENT:12 DATE:2026-05-19

use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::task::JoinHandle;

#[tokio::test(flavor = "multi_thread")]
async fn test_multi_collection_stress() {
    let tmp = TempDir::new().expect("failed to create temp dir");
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

    let num_tasks = 30;
    let ops_per_task = 40;
    let num_collections = 10;
    let mut handles: Vec<JoinHandle<()>> = Vec::new();

    for t in 0..num_tasks {
        let db = db.clone();
        handles.push(tokio::spawn(async move {
            for i in 0..ops_per_task {
                let col_idx = (t + i) % num_collections;
                let col_name = format!("col-{}", col_idx);
                let col = db.collection(&col_name).await.expect("collection");

                let id = format!("task-{}-doc-{}", t, i);
                let vec = vec![t as f32, i as f32, col_idx as f32, 1.0];

                // 1. Insert
                col.insert(&id, &vec, Some(json!({"t": t, "i": i, "c": col_idx})))
                    .await
                    .expect("insert");

                // 2. Search
                let results = col.search(&vec, 1).await.expect("search");
                assert!(!results.is_empty());

                // 3. Optional: small scan
                let _ = col.scan_prefix(&format!("task-{}", t)).await.expect("scan");

                // 4. Delete every other doc
                if i % 2 == 0 {
                    col.delete(&id).await.expect("delete");
                }
            }
        }));
    }

    for h in handles {
        h.await.expect("task failed");
    }

    // Final check
    let mut total_docs = 0;
    for c in 0..num_collections {
        let col = db.collection(&format!("col-{}", c)).await.expect("collection");
        total_docs += col.len().await;
    }

    assert_eq!(total_docs, (num_tasks * ops_per_task) / 2);
}
