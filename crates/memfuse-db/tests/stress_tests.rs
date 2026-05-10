use memfuse_db::{MemFuse, MemFuseConfig, json};
use tempfile::TempDir;
use std::sync::Arc;
use tokio::task;

#[tokio::test]
async fn test_concurrent_stress_ops() {
    let tmp = TempDir::new().unwrap();
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = Arc::new(MemFuse::open_with_config(tmp.path(), config).await.unwrap());

    let num_tasks = 10;
    let ops_per_task = 20;
    let mut handles = Vec::new();

    for i in 0..num_tasks {
        let db_clone = Arc::clone(&db);
        let handle = task::spawn(async move {
            for j in 0..ops_per_task {
                let id = format!("task-{}-doc-{}", i, j);
                let vec = vec![i as f32, j as f32, 0.0, 0.0];

                // Insert
                db_clone.insert(&id, &vec, Some(json!({"task": i, "op": j}))).await.unwrap();

                // Search
                let results = db_clone.search(&vec, 1).await.unwrap();
                assert!(!results.is_empty());

                // Delete some
                if j % 2 == 0 {
                    db_clone.delete(&id).await.unwrap();
                }
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    // Am Ende: Verify Konsistenz
    // Half were deleted
    let expected_count = (num_tasks * ops_per_task) / 2;
    let actual_count = db.len().await.unwrap();
    assert_eq!(actual_count, expected_count);
}
