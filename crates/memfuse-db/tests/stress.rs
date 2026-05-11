use memfuse_db::{MemFuse, MemFuseConfig};
use tempfile::TempDir;
use std::sync::Arc;
use tokio::task::JoinHandle;

// ANCHOR:INTEGRATION:STRESS-001 — Concurrent Ingest Stress Test
// AGENT:12 DATE:2026-05-09 STATUS:DONE
#[tokio::test]
async fn test_stress_concurrent_inserts() {
    let tmp = TempDir::new().unwrap();
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = Arc::new(MemFuse::open_with_config(tmp.path(), config).await.unwrap());

    let num_tasks = 10;
    let docs_per_task = 50;
    let mut handles: Vec<JoinHandle<()>> = Vec::new();

    for t in 0..num_tasks {
        let db_clone = db.clone();
        handles.push(tokio::spawn(async move {
            let col = db_clone.collection("stress_col").await.unwrap();
            for i in 0..docs_per_task {
                let id = format!("task-{}-doc-{}", t, i);
                col.insert(&id, &[t as f32, i as f32, 0.0, 0.0], None).await.unwrap();
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    let col = db.collection("stress_col").await.unwrap();
    assert_eq!(col.len().await, num_tasks * docs_per_task);

    // Concurrent Search & Delete
    let mut search_handles = Vec::new();
    for t in 0..num_tasks {
        let db_clone = db.clone();
        search_handles.push(tokio::spawn(async move {
            let col = db_clone.collection("stress_col").await.unwrap();
            for i in 0..docs_per_task {
                let id = format!("task-{}-doc-{}", t, i);
                let _ = col.get(&id).await.unwrap().expect("Should find doc");
                col.delete(&id).await.unwrap();
            }
        }));
    }

    for h in search_handles {
        h.await.unwrap();
    }

    assert_eq!(col.len().await, 0);
}
