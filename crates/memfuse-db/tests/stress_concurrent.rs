use memfuse_db::{json, DistanceMetric, MemFuse, MemFuseConfig};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::task;

#[tokio::test]
async fn test_concurrent_stress_ops() {
    let tmp = TempDir::new().unwrap();
    let config = MemFuseConfig {
        dimension: 4,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };

    let db = Arc::new(MemFuse::open_with_config(tmp.path(), config).await.unwrap());

    let num_tasks = 10;
    let ops_per_task = 50;
    let mut handles = Vec::new();

    for t in 0..num_tasks {
        let db = Arc::clone(&db);
        let handle = task::spawn(async move {
            let col_name = if t % 2 == 0 { "default" } else { "named_col" };
            let col = db.collection(col_name).await.unwrap();

            for i in 0..ops_per_task {
                let id = format!("task-{}-doc-{}", t, i);
                let embedding = vec![t as f32, i as f32, 0.0, 0.0];

                // 1. Insert
                col.insert(&id, &embedding, Some(json!({"t": t, "i": i})))
                    .await
                    .unwrap();

                // 2. Search (sometimes)
                if i % 10 == 0 {
                    let results = col.search(&embedding, 5).await.unwrap();
                    assert!(!results.is_empty());
                }

                // 3. Update
                col.update(
                    &id,
                    &embedding,
                    Some(json!({"t": t, "i": i, "updated": true})),
                )
                .await
                .unwrap();

                // 4. Delete
                col.delete(&id).await.unwrap();
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    // Verify database is empty at the end
    let col_default = db.collection("default").await.unwrap();
    let col_named = db.collection("named_col").await.unwrap();

    assert_eq!(col_default.len().await, 0);
    assert_eq!(col_named.len().await, 0);
}
