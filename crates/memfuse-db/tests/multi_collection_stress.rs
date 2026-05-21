//! Multi-collection concurrency stress tests for MemFuse.
// ANCHOR:INTEGRATION:STRESS-002 STATUS:READY AGENT:12 DATE:2026-05-21

use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::task::JoinHandle;

#[tokio::test(flavor = "multi_thread")]
async fn test_multi_collection_concurrency() {
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

    let num_collections = 10;
    let tasks_per_col = 5;
    let ops_per_task = 20;
    let mut handles: Vec<JoinHandle<()>> = Vec::new();

    for c in 0..num_collections {
        let col_name = format!("collection-{}", c);
        let col = Arc::new(db.collection(&col_name).await.expect("collection"));

        for t in 0..tasks_per_col {
            let col = col.clone();
            let col_id = c;
            let task_id = t;

            handles.push(tokio::spawn(async move {
                for i in 0..ops_per_task {
                    let doc_id = format!("col-{}-task-{}-doc-{}", col_id, task_id, i);
                    let vec = vec![col_id as f32, task_id as f32, i as f32, 0.0];

                    // Insert
                    col.insert(
                        &doc_id,
                        &vec,
                        Some(json!({"c": col_id, "t": task_id, "i": i})),
                    )
                    .await
                    .expect("insert");

                    // Search
                    let results = col.search(&vec, 3).await.expect("search");
                    assert!(
                        !results.is_empty(),
                        "Search in collection-{} should find results",
                        col_id
                    );

                    // Verify first result is likely the inserted doc (or at least same col)
                    let meta = results[0].metadata.as_ref().expect("metadata");
                    assert_eq!(meta["c"], col_id, "Result from wrong collection!");

                    // Delete every other doc
                    if i % 2 == 0 {
                        col.delete(&doc_id).await.expect("delete");
                    }
                }
            }));
        }
    }

    for h in handles {
        h.await.expect("task panicked");
    }

    // Final validation
    for c in 0..num_collections {
        let col_name = format!("collection-{}", c);
        let col = db.collection(&col_name).await.expect("collection");
        let expected_len = (tasks_per_col * ops_per_task) / 2;
        let actual_len = col.len().await;
        assert_eq!(
            actual_len, expected_len,
            "Collection {} length mismatch",
            col_name
        );
    }

    // Verify list_collections
    let list = db.list_collections().await.expect("list");
    assert_eq!(list.len(), num_collections + 1); // +1 for "default"
}
