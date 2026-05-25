//! Stress tests for the full MemFuse stack.
// ANCHOR:INTEGRATION:STRESS-001 STATUS:READY AGENT:12 DATE:2026-06-10

use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_concurrent_multi_collection_operations() {
    let tmp = TempDir::new().expect("Failed to create temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        max_elements: 10000,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };

    let db = Arc::new(MemFuse::open_with_config(tmp.path(), config).await.expect("Failed to open DB"));

    let mut tasks = Vec::new();
    let num_tasks = 10;
    let ops_per_task = 50;

    for t in 0..num_tasks {
        let db = db.clone();
        tasks.push(tokio::spawn(async move {
            // Half of tasks use a shared collection, half use unique ones
            let col_name = if t % 2 == 0 {
                "shared_col".to_string()
            } else {
                format!("unique_col_{}", t)
            };

            let col = db.collection(&col_name).await.expect("Failed to get collection");

            for i in 0..ops_per_task {
                let id = format!("task-{}-doc-{}", t, i);
                // Normalized vectors for Cosine similarity
                let mut vec = vec![0.0; 4];
                vec[0] = (t + 1) as f32;
                vec[1] = (i + 1) as f32;
                let norm = vec.iter().map(|x| x*x).sum::<f32>().sqrt();
                for x in &mut vec { *x /= norm; }

                // 1. Insert/Upsert
                col.upsert(&id, &vec, Some(json!({"task": t, "i": i})))
                    .await
                    .expect("Upsert failed");

                // 2. Search
                let results = col.search(&vec, 10).await.expect("Search failed");
                assert!(!results.is_empty(), "Search results empty for task {}, doc {}", t, i);
                // The first result should be the one we just inserted or something very close
                assert!(results.iter().any(|r| r.id == id), "Doc {} not found in search results for task {}", id, t);

                // 3. Relate (every 5th op)
                if i % 5 == 0 && i > 0 {
                    let prev_id = format!("task-{}-doc-{}", t, i - 1);
                    col.relate(&id, &prev_id, "sequence")
                        .await
                        .expect("Relate failed");
                }

                // 4. Delete (every 10th op, delete the 5th previous one)
                // For i in 0..50: 10, 20, 30, 40 -> 4 deletes.
                if i % 10 == 0 && i > 0 {
                    let del_id = format!("task-{}-doc-{}", t, i - 5);
                    col.delete(&del_id).await.expect("Delete failed");
                }
            }
        }));
    }

    for task in tasks {
        task.await.expect("Task failed");
    }

    // Final consistency check
    let shared_col = db.collection("shared_col").await.expect("Failed to get shared collection");

    // We inserted ops_per_task * (num_tasks / 2) = 50 * 5 = 250 docs into shared_col
    // We deleted for i in {10, 20, 30, 40} -> 4 deletes per task.
    // Total deletes = 4 * 5 = 20.
    // Expected len = 250 - 20 = 230.
    assert_eq!(shared_col.len().await, 230);

    for t in (1..num_tasks).step_by(2) {
        let col_name = format!("unique_col_{}", t);
        let col = db.collection(&col_name).await.expect("Failed to get collection");
        // 50 inserts - 4 deletes = 46
        assert_eq!(col.len().await, 46);
    }
}
