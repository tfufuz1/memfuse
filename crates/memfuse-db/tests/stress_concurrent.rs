//! Concurrent Stress Test for MemFuse.
use memfuse_db::{MemFuse, MemFuseConfig};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::task::JoinHandle;

#[tokio::test]
async fn test_stress_concurrent_ops() {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = Arc::new(
        MemFuse::open_with_config(tmp.path(), config)
            .await
            .expect("open db"),
    );

    let num_tasks = 10;
    let ops_per_task = 20;
    let mut handles: Vec<JoinHandle<()>> = Vec::new();

    for t_idx in 0..num_tasks {
        let db = db.clone();
        handles.push(tokio::spawn(async move {
            let col = db
                .collection(&format!("col-{}", t_idx % 2))
                .await
                .expect("get col");

            for op_idx in 0..ops_per_task {
                let id = format!("task-{}-op-{}", t_idx, op_idx);
                let embedding = vec![t_idx as f32, op_idx as f32, 0.0, 0.0];

                // 1. Insert
                col.insert(
                    &id,
                    &embedding,
                    Some(json!({"t": t_idx, "op": op_idx, "text": "stress test keyword"})),
                )
                .await
                .expect("insert");

                // 2. Search (Vector)
                let res = col.search(&embedding, 1).await.expect("search");
                assert!(
                    !res.is_empty(),
                    "Should find at least one doc in col-{}",
                    t_idx % 2
                );

                // 3. Search (Hybrid)
                let hybrid = col
                    .hybrid_search("keyword", &embedding, 5)
                    .await
                    .expect("hybrid search");
                assert!(!hybrid.is_empty());

                // 4. Update
                col.update(&id, &embedding, Some(json!({"status": "updated"})))
                    .await
                    .expect("update");

                // 5. Get
                let doc = col.get(&id).await.expect("get").expect("exists");
                assert_eq!(doc.metadata.unwrap()["status"], "updated");

                // 6. Delete half of them
                if op_idx % 2 == 0 {
                    col.delete(&id).await.expect("delete");
                    assert!(col.get(&id).await.expect("get after delete").is_none());
                }
            }
        }));
    }

    for h in handles {
        h.await.expect("task failed");
    }

    // Final consistency check
    let col0 = db.collection("col-0").await.expect("col-0");
    let col1 = db.collection("col-1").await.expect("col-1");

    // Each collection should have half of (num_tasks/2 * ops_per_task) documents
    // num_tasks = 10 -> 5 tasks per collection
    // ops_per_task = 20 -> 10 deleted, 10 remain per task
    // Total should be 50 per collection
    assert_eq!(col0.len().await, 50);
    assert_eq!(col1.len().await, 50);
}
