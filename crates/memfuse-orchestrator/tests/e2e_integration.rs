use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::task::JoinHandle;

#[tokio::test]
async fn test_e2e_agent_workflow() {
    // 1. MemFuse::open()
    let tmp = TempDir::new().expect("failed to create temp dir");
    let config = MemFuseConfig {
        dimension: 3,
        max_elements: 1000,
        distance_metric: DistanceMetric::Cosine,
        encryption_passphrase: None,
    };
    let db = MemFuse::open_with_config(tmp.path(), config)
        .await
        .expect("failed to open db");

    // 2. Insert Dokumente mit Embeddings + Metadata
    db.insert(
        "agent-1",
        &[1.0, 0.0, 0.0],
        Some(serde_json::json!({"name": "Agent 1"})),
    )
    .await
    .expect("insert");
    db.insert(
        "agent-2",
        &[0.0, 1.0, 0.0],
        Some(serde_json::json!({"name": "Agent 2"})),
    )
    .await
    .expect("insert");

    // 3. Search
    let results = db.search(&[1.0, 0.1, 0.0], 1).await.expect("search");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "agent-1");
}

#[tokio::test]
async fn test_concurrent_load() {
    // 1. Spawn N tokio::tasks
    // 2. Jede Task: Insert -> Search -> Delete
    // 3. Am Ende: Verify Konsistenz (len == 0)

    let tmp = TempDir::new().expect("failed to create temp dir");
    let db = Arc::new(
        MemFuse::open_with_config(
            tmp.path(),
            MemFuseConfig {
                dimension: 4,
                max_elements: 10000,
                distance_metric: DistanceMetric::Cosine,
                encryption_passphrase: None,
            },
        )
        .await
        .expect("open db"),
    );

    let num_tasks = 20;
    let ops_per_task = 20;
    let mut handles: Vec<JoinHandle<()>> = Vec::new();

    for t in 0..num_tasks {
        let db_clone = Arc::clone(&db);
        handles.push(tokio::spawn(async move {
            for i in 0..ops_per_task {
                let id = format!("stress-{}-{}", t, i);
                let vec = vec![1.0, 0.0, 0.0, 0.0];

                db_clone.insert(&id, &vec, None).await.expect("insert");

                let res = db_clone.search(&vec, 1).await.expect("search");
                assert!(!res.is_empty());

                db_clone.delete(&id).await.expect("delete");
            }
        }));
    }

    for h in handles {
        h.await.expect("task failed");
    }

    let final_len = db.len().await.expect("len");
    assert_eq!(final_len, 0);
}
