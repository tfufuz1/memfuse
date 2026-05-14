//! Concurrent Stress Test for MemFuse.
//! AGENT:12 DATE:2026-05-16 STATUS:READY

use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test(flavor = "multi_thread")]
async fn test_concurrent_load() {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        max_elements: 1000,
        distance_metric: DistanceMetric::Cosine,
    };
    let db = Arc::new(
        MemFuse::open_with_config(tmp.path(), config)
            .await
            .expect("open db"),
    );

    let num_tasks = 8;
    let ops_per_task = 20;
    let mut handles = Vec::new();

    for t in 0..num_tasks {
        let db = db.clone();
        handles.push(tokio::spawn(async move {
            let col_name = format!("stress-col-{}", t);
            let col = db.collection(&col_name).await.expect("get col");

            for i in 0..ops_per_task {
                let id = format!("doc-{}", i);
                let vec = vec![i as f32, (i + 1) as f32, (i + 2) as f32, (i + 3) as f32];

                // 1. Insert
                col.insert(&id, &vec, Some(json!({"t": t, "i": i})))
                    .await
                    .expect("insert");

                // 2. Search
                let res = col.search(&vec, 1).await.expect("search");
                assert!(!res.is_empty());
                assert_eq!(res[0].id, id);

                // 3. Delete
                col.delete(&id).await.expect("delete");
                let doc = col.get(&id).await.expect("get");
                assert!(doc.is_none());
            }
        }));
    }

    for h in handles {
        h.await.expect("task failed");
    }

    // Final consistency check
    for t in 0..num_tasks {
        let col_name = format!("stress-col-{}", t);
        let col = db.collection(&col_name).await.expect("get col");
        assert_eq!(
            col.len().await,
            0,
            "Collection {} should be empty",
            col_name
        );
    }
}
