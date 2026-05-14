//! Stress tests for multiple collections and concurrency.

use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::task::JoinHandle;

#[tokio::test(flavor = "multi_thread")]
async fn test_stress_multi_collection_concurrency() {
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

    let num_tasks = 8;
    let ops_per_task = 40;
    let mut handles: Vec<JoinHandle<()>> = Vec::new();

    for t in 0..num_tasks {
        let db = db.clone();
        handles.push(tokio::spawn(async move {
            let col_name = format!("stress-col-{}", t);
            let col = db.collection(&col_name).await.expect("collection");

            for i in 0..ops_per_task {
                let id = format!("doc-{}-{}", t, i);
                let vec = vec![i as f32, (i + 1) as f32, (i + 2) as f32, (i + 3) as f32];

                // 1. Insert
                col.insert(
                    &id,
                    &vec,
                    Some(json!({"task": t, "idx": i, "text": format!("content for {} {}", t, i)})),
                )
                .await
                .expect("insert");

                // 2. Hybrid Search
                let results = col
                    .hybrid_search(&format!("content for {} {}", t, i), &vec, 1)
                    .await
                    .expect("hybrid search");
                assert!(!results.is_empty());
                assert_eq!(results[0].id, id);

                // 3. Relate (internal to collection)
                if i > 0 {
                    let prev_id = format!("doc-{}-{}", t, i - 1);
                    col.relate(&prev_id, &id, "next").await.expect("relate");
                }

                // 4. Update (occasionally)
                if i % 5 == 0 {
                    col.update(&id, &vec, Some(json!({"updated": true})))
                        .await
                        .expect("update");
                }
            }

            // Verify count
            assert_eq!(col.len().await, ops_per_task);
        }));
    }

    for h in handles {
        h.await.expect("task panicked");
    }

    // Final sanity: all collections present
    let list = db.list_collections().await.expect("list");
    for t in 0..num_tasks {
        assert!(list.contains(&format!("stress-col-{}", t)));
    }
}
