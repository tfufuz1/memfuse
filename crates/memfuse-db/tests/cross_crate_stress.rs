// AGENT:12 DATE:2026-06-25 STATUS:DONE
// ANCHOR:INTEGRATION:STRESS-002 — Cross-Crate Concurrent Stress Test

use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::task;

#[tokio::test]
async fn test_cross_crate_concurrent_load() {
    let tmp = TempDir::new().expect("Failed to create temp dir");
    let config = MemFuseConfig {
        dimension: 8,
        max_elements: 1000,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };

    let db = Arc::new(
        MemFuse::open_with_config(tmp.path(), config)
            .await
            .expect("Failed to open DB"),
    );

    let num_tasks = 5;
    let ops_per_task = 20;
    let mut handles = Vec::new();

    for t_idx in 0..num_tasks {
        let db_clone = Arc::clone(&db);
        let handle = task::spawn(async move {
            let col_name = format!("stress-col-{}", t_idx % 2); // 2 collections shared across 5 tasks
            let col = db_clone.collection(&col_name).await.expect("collection error");

            for i in 0..ops_per_task {
                let doc_id = format!("task-{}-doc-{}", t_idx, i);
                let vec = vec![i as f32; 8];

                // 1. Insert
                col.insert(
                    &doc_id,
                    &vec,
                    Some(json!({"task": t_idx, "val": i, "text": "stress test document"})),
                )
                .await
                .expect("insert error");

                // 2. Hybrid Search - querying for text that matches
                let search_results = col.hybrid_search("stress", &vec, 1).await.expect("search error");
                assert!(!search_results.is_empty(), "Search results empty for task {} iteration {}", t_idx, i);

                // 3. Update
                col.update(
                    &doc_id,
                    &vec,
                    Some(json!({"task": t_idx, "val": i, "updated": true, "text": "updated stress test document"})),
                )
                .await
                .expect("update error");

                // 4. Relate
                if i > 0 {
                    let prev_doc_id = format!("task-{}-doc-{}", t_idx, i - 1);
                    col.relate(&doc_id, &prev_doc_id, "prev").await.expect("relate error");
                }
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.expect("Task panicked");
    }

    // Final verification
    for t_idx in 0..2 {
        let col_name = format!("stress-col-{}", t_idx);
        let _col = db.collection(&col_name).await.expect("final col error");
    }

    let stats = db.stats().await.expect("stats error");
    assert!(stats.storage_stats.memtable_size_bytes > 0);
}

#[tokio::test]
async fn test_high_concurrency_collection_creation() {
    let tmp = TempDir::new().expect("Failed to create temp dir");
    let db = Arc::new(MemFuse::open(tmp.path()).await.expect("Failed to open DB"));

    let num_collections = 20;
    let mut handles = Vec::new();

    for i in 0..num_collections {
        let db_clone = Arc::clone(&db);
        handles.push(task::spawn(async move {
            let name = format!("col-{}", i);
            let _col = db_clone.collection(&name).await.expect("failed to create col");
        }));
    }

    for handle in handles {
        handle.await.expect("Creation task failed");
    }

    let collections = db.list_collections().await.expect("list failed");
    // default + num_collections
    assert_eq!(collections.len(), num_collections + 1);
}
