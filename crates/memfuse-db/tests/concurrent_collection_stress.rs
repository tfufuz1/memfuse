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
                // Ensure unique, non-zero vectors to avoid collisions in search results
                let vec = vec![t as f32 + 1.0, i as f32 + 1.0, (t * i) as f32, 1.0];

                // 1. Insert
                col.insert(&id, &vec, Some(json!({"t": t, "i": i})))
                    .await
                    .expect("insert");

                // 2. Get
                let doc = col.get(&id).await.expect("get").expect("should exist");
                assert_eq!(doc.metadata.unwrap()["i"], i);

                // 3. Update
                col.update(&id, &vec, Some(json!({"t": t, "i": i, "updated": true})))
                    .await
                    .expect("update");

                // 4. Search
                let results = col.search(&vec, 1).await.expect("search");
                assert!(
                    !results.is_empty(),
                    "Search should find at least one result (itself)"
                );
                assert_eq!(results[0].id, id);
                assert!(results[0].metadata.as_ref().unwrap()["updated"]
                    .as_bool()
                    .unwrap());

                // 5. Delete
                col.delete(&id).await.expect("delete");

                // 6. Verify Gone
                let doc = col.get(&id).await.expect("get");
                assert!(doc.is_none());
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
