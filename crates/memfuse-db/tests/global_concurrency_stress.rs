// ANCHOR:INTEGRATION:STRESS-001 STATUS:READY AGENT:12
use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test(flavor = "multi_thread")]
async fn test_global_concurrency_stress() {
    let tmp = TempDir::new().expect("failed to create temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        max_elements: 1000,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };
    let db = Arc::new(
        MemFuse::open_with_config(tmp.path(), config)
            .await
            .expect("open db"),
    );

    let n_tasks = 10;
    let ops_per_task = 50;
    let mut handles = Vec::new();

    for t in 0..n_tasks {
        let db = db.clone();
        handles.push(tokio::spawn(async move {
            let col_name = format!("stress-col-{}", t);
            let col = db.collection(&col_name).await.expect("collection");

            for i in 0..ops_per_task {
                let id = format!("doc-{}", i);
                let vec = vec![t as f32, i as f32, 0.0, 0.0];

                // Insert -> Search -> Delete
                col.insert(&id, &vec, Some(json!({"t": t, "i": i})))
                    .await
                    .expect("insert");
                let res = col.search(&vec, 1).await.expect("search");
                assert_eq!(res[0].id, id);
                col.delete(&id).await.expect("delete");
            }
        }));
    }

    for h in handles {
        h.await.expect("task panicked");
    }

    // Final verify consistency
    for t in 0..n_tasks {
        let col_name = format!("stress-col-{}", t);
        let col = db.collection(&col_name).await.expect("collection");
        assert_eq!(col.len().await, 0);
    }
}
