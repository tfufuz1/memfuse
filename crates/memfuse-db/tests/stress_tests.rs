use memfuse_db::{MemFuse, MemFuseConfig};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::task::JoinHandle;

#[tokio::test]
async fn test_concurrent_load_stress() {
    let tmp = TempDir::new().unwrap();
    let config = MemFuseConfig {
        dimension: 4,
        max_elements: 10000,
        ..Default::default()
    };
    let db = Arc::new(MemFuse::open_with_config(tmp.path(), config).await.expect("open db"));

    let num_tasks = 10;
    let ops_per_task = 50;
    let mut handles: Vec<JoinHandle<()>> = Vec::new();

    for t in 0..num_tasks {
        let db = db.clone();
        let handle = tokio::spawn(async move {
            let col_name = format!("col-{}", t % 3); // Use 3 different collections
            let col = db.collection(&col_name).await.expect("get col");

            for i in 0..ops_per_task {
                let id = format!("task-{}-doc-{}", t, i);
                let embedding = vec![t as f32, i as f32, 0.0, 0.0];

                // 1. Insert
                col.insert(&id, &embedding, Some(json!({"t": t, "i": i})))
                    .await
                    .expect("insert failed");

                // 2. Search (occasionally)
                if i % 10 == 0 {
                    let results = col.search(&embedding, 5).await.expect("search failed");
                    assert!(!results.is_empty());
                }

                // 3. Update (occasionally)
                if i % 15 == 0 {
                    col.update(&id, &embedding, Some(json!({"t": t, "i": i, "updated": true})))
                        .await
                        .expect("update failed");
                }
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.expect("task panicked");
    }

    // Verify consistency
    let mut total_count = 0;
    for t in 0..3 {
        let col = db.collection(&format!("col-{}", t)).await.expect("get col");
        total_count += col.len().await;
    }

    assert_eq!(total_count, num_tasks * ops_per_task);

    // Final search test
    let col = db.collection("col-0").await.expect("get col");
    let stats = col.stats().await.expect("stats");
    assert!(stats.num_vectors > 0);
}
