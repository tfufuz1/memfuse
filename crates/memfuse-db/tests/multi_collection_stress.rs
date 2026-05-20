//! Stress test for concurrent operations across multiple collections.
// ANCHOR:INTEGRATION:STRESS-002 STATUS:READY AGENT:12 DATE:2026-05-18

use memfuse_db::{MemFuse, MemFuseConfig};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::task::JoinHandle;

#[tokio::test(flavor = "multi_thread")]
async fn test_multi_collection_concurrency_stress() {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 2,
        max_elements: 5000,
        ..Default::default()
    };
    let db = Arc::new(
        MemFuse::open_with_config(tmp.path(), config)
            .await
            .expect("open db"),
    );

    let num_collections = 10;
    let tasks_per_col = 3;
    let ops_per_task = 30;
    let mut handles: Vec<JoinHandle<()>> = Vec::new();

    for c in 0..num_collections {
        let col_name = format!("stress-col-{}", c);
        for t in 0..tasks_per_col {
            let db = db.clone();
            let col_name = col_name.clone();
            handles.push(tokio::spawn(async move {
                let col = db.collection(&col_name).await.expect("collection failed");
                for i in 0..ops_per_task {
                    let id = format!("task-{}-doc-{}", t, i);
                    let vec = vec![c as f32 + 10.0, (t * 100 + i) as f32];

                    // Insert
                    col.insert(&id, &vec, Some(json!({"col": c, "task": t, "i": i})))
                        .await
                        .expect("insert failed");

                    // Small random delay could be here, but tokio schedule is enough

                    // Search
                    let results = col.search(&vec, 10).await.expect("search failed");
                    assert!(!results.is_empty(), "Search for {:?} in {} (task {}) returned no results", vec, col_name, t);
                    // Check if it's our doc
                    let found_ids: Vec<String> = results.iter().map(|r| r.id.clone()).collect();
                    assert!(found_ids.contains(&id), "Doc {} not found in results for task {} in {}. Found: {:?}", id, t, col_name, found_ids);
                }
            }));
        }
    }

    for h in handles {
        h.await.expect("task panicked");
    }

    // Verify all collections have correct count
    for c in 0..num_collections {
        let col_name = format!("stress-col-{}", c);
        let col = db.collection(&col_name).await.expect("collection failed");
        assert_eq!(col.len().await, (tasks_per_col * ops_per_task) as usize);
    }
}
