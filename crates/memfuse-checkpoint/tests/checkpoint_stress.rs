//! Stress tests for MemFuse checkpoints and forks.
// ANCHOR:INTEGRATION:CHECKPOINT-STRESS-001 STATUS:REVIEW AGENT:12 DATE:2026-05-22

use memfuse_checkpoint::CheckpointManager;
use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::task::JoinHandle;

#[tokio::test(flavor = "multi_thread")]
async fn test_concurrent_checkpoint_fork_stress() {
    let tmp = TempDir::new().expect("failed to create temp dir");
    let db_path = tmp.path().to_path_buf();

    let config = MemFuseConfig {
        dimension: 4,
        max_elements: 10000,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };

    // 1. Setup DB and write initial data
    let db = Arc::new(
        MemFuse::open_with_config(&db_path, config.clone())
            .await
            .expect("open db"),
    );
    let main_col = db.collection("main").await.expect("main col");

    for i in 0..100 {
        main_col
            .insert(
                &format!("init-{}", i),
                &[i as f32, 0.0, 0.0, 0.0],
                Some(json!({"version": "initial"})),
            )
            .await
            .expect("insert failed");
    }

    // 2. Stress Test: Concurrently create checkpoints while writing more data
    let num_tasks = 5;
    let mut handles: Vec<JoinHandle<()>> = Vec::new();

    let cp_manager = Arc::new(CheckpointManager::new(db.inner_storage()));

    for t in 0..num_tasks {
        let main_col = main_col.clone();
        let cp_manager = cp_manager.clone();
        let db = db.clone();

        handles.push(tokio::spawn(async move {
            for i in 0..20 {
                // Write data
                let id = format!("task-{}-doc-{}", t, i);
                main_col
                    .insert(
                        &id,
                        &[t as f32, i as f32, 0.0, 0.0],
                        Some(json!({"task": t, "i": i})),
                    )
                    .await
                    .expect("insert");

                // Occasionally create a "fork" collection
                if i % 5 == 0 {
                    let cp_name = format!("cp-{}-{}", t, i);
                    let cp = cp_manager
                        .create_checkpoint(&cp_name)
                        .await
                        .expect("checkpoint failed");

                    let fork_name = format!("fork-{}-{}", t, i);
                    let fork_col = db.collection(&fork_name).await.expect("fork col failed");

                    // Simple "fork" logic: copy 10 random docs from main
                    let docs = main_col.scan_prefix("init-").await.expect("scan failed");
                    for (doc_id, meta) in docs.into_iter().take(10) {
                        fork_col
                            .insert(&doc_id, &[1.0, 1.0, 1.0, 1.0], Some(meta))
                            .await
                            .expect("fork insert");
                    }

                    cp_manager
                        .drop_checkpoint(&cp)
                        .await
                        .expect("drop checkpoint failed");
                }
            }
        }));
    }

    for h in handles {
        h.await.expect("task failed");
    }

    // 3. Final Verification
    let list = db
        .list_collections()
        .await
        .expect("list collections failed");
    assert!(list.contains(&"main".to_string()));
    assert!(list.len() > num_tasks);

    let final_len = main_col.len().await;
    assert_eq!(final_len, 100 + (num_tasks * 20));
}
