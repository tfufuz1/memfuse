//! High-concurrency multi-collection stress tests for MemFuse.
// ANCHOR:INTEGRATION:STRESS-002 STATUS:READY AGENT:12 DATE:2026-05-21

use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::task::JoinHandle;

#[tokio::test(flavor = "multi_thread")]
async fn test_multi_collection_stress() {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        max_elements: 100000,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };
    let db = Arc::new(
        MemFuse::open_with_config(tmp.path(), config)
            .await
            .expect("open db"),
    );

    let num_collections = 5;
    let num_tasks = 40;
    let ops_per_task = 100;
    let mut handles: Vec<JoinHandle<()>> = Vec::new();

    for t in 0..num_tasks {
        let db = db.clone();
        let collection_name = format!("col-{}", t % num_collections);

        handles.push(tokio::spawn(async move {
            let col = db.collection(&collection_name).await.expect("get collection");

            for i in 0..ops_per_task {
                let id = format!("task-{}-doc-{}", t, i);
                // Unique direction for each task/op to avoid Cosine collisions
                let vec = vec![1.0, (t + 1) as f32, (i + 1) as f32, (t * i) as f32];

                // 1. Insert
                col.insert(&id, &vec, Some(json!({"t": t, "i": i, "col": collection_name})))
                    .await
                    .expect("insert");

                // 2. Search
                let results = col.search(&vec, 1).await.expect("search");
                assert!(
                    !results.is_empty(),
                    "Search should find at least one result (itself) in col {}",
                    collection_name
                );
                // We don't strictly assert_eq!(results[0].id, id) here because with Cosine distance
                // and concurrent inserts, another document might be extremely close.
                // But with our new unique vectors it should mostly work.
                // Let's at least check if our ID is in the top results if we search for more.
                let results_k = col.search(&vec, 5).await.expect("search k");
                assert!(
                    results_k.iter().any(|r| r.id == id),
                    "Doc {} not found in search results for col {}",
                    id, collection_name
                );

                // 3. Delete
                col.delete(&id).await.expect("delete");

                // 4. Verify gone
                let doc = col.get(&id).await.expect("get");
                assert!(doc.is_none(), "Doc {} should be deleted from col {}", id, collection_name);
            }
        }));
    }

    for h in handles {
        h.await.expect("task panicked");
    }

    // Final consistency check: all collections should be empty
    for i in 0..num_collections {
        let name = format!("col-{}", i);
        let col = db.collection(&name).await.expect("get collection");
        let final_len = col.len().await;
        assert_eq!(
            final_len, 0,
            "Collection {} should be empty, but has {} docs",
            name, final_len
        );
    }
}
