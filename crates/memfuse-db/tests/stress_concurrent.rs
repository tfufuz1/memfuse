use memfuse_db::{MemFuse, MemFuseConfig};
use tempfile::TempDir;
use serde_json::json;
use std::sync::Arc;
use tokio::task::JoinSet;

#[tokio::test]
async fn test_stress_concurrent_ops() {
    let tmp = TempDir::new().unwrap();
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = Arc::new(MemFuse::open_with_config(tmp.path(), config).await.unwrap());

    let num_tasks = 8;
    let ops_per_task = 50;
    let mut set = JoinSet::new();

    for task_id in 0..num_tasks {
        let db = Arc::clone(&db);
        set.spawn(async move {
            let col = db.collection("stress").await.unwrap();
            for op_id in 0..ops_per_task {
                let doc_id = format!("task-{}-doc-{}", task_id, op_id);
                let embedding = vec![task_id as f32, op_id as f32, 0.0, 0.0];

                // 1. Insert
                col.insert(&doc_id, &embedding, Some(json!({"task": task_id, "op": op_id})))
                    .await
                    .expect("Insert failed during stress test");

                // 2. Search
                let results = col.search(&embedding, 5).await.expect("Search failed during stress test");
                assert!(!results.is_empty(), "Search should return at least one result");

                // 3. Relate (internal to same task docs if op_id > 0)
                if op_id > 0 {
                    let prev_id = format!("task-{}-doc-{}", task_id, op_id - 1);
                    col.relate(&doc_id, &prev_id, "previous").await.expect("Relate failed during stress test");
                }
            }
        });
    }

    // Wait for all insert/search/relate tasks to complete
    while let Some(res) = set.join_next().await {
        res.expect("Task panicked");
    }

    let col = db.collection("stress").await.unwrap();
    assert_eq!(col.len().await, num_tasks * ops_per_task);

    // Now spawn concurrent delete tasks
    for task_id in 0..num_tasks {
        let db = Arc::clone(&db);
        set.spawn(async move {
            let col = db.collection("stress").await.unwrap();
            for op_id in 0..ops_per_task {
                let doc_id = format!("task-{}-doc-{}", task_id, op_id);
                col.delete(&doc_id).await.expect("Delete failed during stress test");
            }
        });
    }

    while let Some(res) = set.join_next().await {
        res.expect("Task panicked");
    }

    assert_eq!(col.len().await, 0);
}

#[tokio::test]
async fn test_concurrent_collection_creation() {
    let tmp = TempDir::new().unwrap();
    let db = Arc::new(MemFuse::open(tmp.path()).await.unwrap());

    let mut set = JoinSet::new();
    let num_tasks = 20;

    for i in 0..num_tasks {
        let db = Arc::clone(&db);
        set.spawn(async move {
            // Multiple tasks trying to create/get the same collection
            let col_name = format!("col-{}", i % 5);
            let _col = db.collection(&col_name).await.unwrap();
        });
    }

    while let Some(res) = set.join_next().await {
        res.expect("Task panicked");
    }

    let collections = db.list_collections().await.unwrap();
    // default + 5 unique named collections
    assert_eq!(collections.len(), 6);
}
