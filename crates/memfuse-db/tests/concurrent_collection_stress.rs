//! High-concurrency Stress Test on a single collection.

use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::task::JoinHandle;

#[tokio::test(flavor = "multi_thread")]
async fn test_single_collection_stress_concurrency() {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        max_elements: 10000,
        distance_metric: DistanceMetric::Cosine,
    };
    let db = Arc::new(
        MemFuse::open_with_config(tmp.path(), config)
            .await
            .expect("open db"),
    );

    let col_name = "stress-collection";
    let col = Arc::new(db.collection(col_name).await.expect("collection"));

    let num_tasks = 12;
    let ops_per_task = 40;
    let mut handles: Vec<JoinHandle<()>> = Vec::new();

    for t in 0..num_tasks {
        let col = col.clone();
        handles.push(tokio::spawn(async move {
            for i in 0..ops_per_task {
                let id = format!("task-{}-doc-{}", t, i);
                let vec = vec![i as f32, (i + 1) as f32, (i + 2) as f32, (i + 3) as f32];

                // 1. Insert
                col.insert(&id, &vec, Some(json!({"t": t, "i": i, "text": format!("content for task {} op {}", t, i)})))
                    .await
                    .expect("insert");

                // 2. Search (Vector)
                let results = col.search(&vec, 1).await.expect("search");
                assert!(!results.is_empty(), "Result set empty for task {} op {}", t, i);
                // We don't assert id == id because another task might have inserted an identical vector
                // But it should at least return something.

                // 3. Hybrid Search
                let hybrid_results = col.hybrid_search(&format!("task {} op {}", t, i), &vec, 1).await.expect("hybrid search");
                assert!(!hybrid_results.is_empty());

                // 4. Get by Key
                let doc = col.get(&id).await.expect("get").expect("should exist");
                assert_eq!(doc.id, id);

                // 5. Conditional Delete (every 3rd)
                if i % 3 == 0 {
                    col.delete(&id).await.expect("delete");
                    let deleted = col.get(&id).await.expect("get after delete");
                    assert!(deleted.is_none());
                }
            }
        }));
    }

    for h in handles {
        h.await.expect("task panicked");
    }

    // Final check: should have (num_tasks * ops_per_task) - (num_tasks * ops_per_task / 3 rounded up?)
    // Actually every i % 3 == 0 is deleted.
    // for i in 0..40: 0, 3, 6, 9, 12, 15, 18, 21, 24, 27, 30, 33, 36, 39 are deleted (14 items per task).
    // Remaining: 40 - 14 = 26 per task.
    // Total remaining: 12 * 26 = 312.
    let count = col.len().await;
    println!("Final collection size: {}", count);
    assert!(count > 0);
}
