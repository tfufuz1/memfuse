use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::task::JoinHandle;

#[tokio::test(flavor = "multi_thread")]
async fn test_stress_concurrent_ops() {
    let tmp = TempDir::new().expect("temp dir");
    let db = Arc::new(
        MemFuse::open_with_config(
            tmp.path(),
            MemFuseConfig {
                dimension: 4,
                max_elements: 10000,
                distance_metric: DistanceMetric::Cosine,
                ..Default::default()
            },
        )
        .await
        .expect("open db"),
    );

    let num_tasks = 20;
    let ops_per_task = 50;
    let mut handles: Vec<JoinHandle<()>> = Vec::new();

    for t in 0..num_tasks {
        let db = db.clone();
        handles.push(tokio::spawn(async move {
            for i in 0..ops_per_task {
                let id = format!("task-{}-doc-{}", t, i);
                let vec = vec![t as f32, i as f32, (t + i) as f32, 0.0];

                // 1. Insert
                db.insert(&id, &vec, Some(json!({"t": t, "i": i})))
                    .await
                    .expect("insert");

                // 2. Search
                let res = db.search(&vec, 1).await.expect("search");
                assert_eq!(res[0].id, id);

                // 3. Delete
                db.delete(&id).await.expect("delete");

                // Verify gone
                let doc = db.get(&id).await.expect("get");
                assert!(doc.is_none());
            }
        }));
    }

    for h in handles {
        h.await.expect("task failed");
    }

    assert_eq!(db.len().await.expect("len"), 0);
}
