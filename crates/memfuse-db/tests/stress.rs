use memfuse_db::{MemFuse, MemFuseConfig};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::task;

#[tokio::test]
async fn test_concurrent_load_stress() {
    let tmp = TempDir::new().unwrap();
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = Arc::new(MemFuse::open_with_config(tmp.path(), config).await.unwrap());

    let num_tasks = 20;
    let ops_per_task = 50;
    let mut handles = Vec::new();

    for t in 0..num_tasks {
        let db = db.clone();
        handles.push(task::spawn(async move {
            for i in 0..ops_per_task {
                let id = format!("task-{}-doc-{}", t, i);
                let embedding = vec![t as f32; 4];

                // Insert
                db.insert(&id, &embedding, None).await.unwrap();

                // Search
                let _ = db.search(&embedding, 1).await.unwrap();

                // Delete
                db.delete(&id).await.unwrap();
            }
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    // Verify consistency: len should be 0 eventually if all deletes succeeded
    assert_eq!(db.len().await.unwrap(), 0);
}
