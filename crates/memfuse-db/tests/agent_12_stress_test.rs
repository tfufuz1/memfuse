//! Stress tests for the MemFuse database with high concurrency.
// ANCHOR:INTEGRATION:STRESS-002 STATUS:READY AGENT:12 DATE:2026-05-24

use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::task::JoinHandle;

#[tokio::test(flavor = "multi_thread")]
async fn test_agent_12_concurrency_and_stress() {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        max_elements: 10000,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };
    let db = Arc::new(
        MemFuse::open_with_config(tmp.path(), config)
            .await
            .expect("open db"),
    );

    let num_tasks = 20;
    let ops_per_task = 50;
    let mut handles: Vec<JoinHandle<()>> = Vec::new();

    for t in 0..num_tasks {
        let db = db.clone();
        handles.push(tokio::spawn(async move {
            let col_name = format!("stress-col-{}", t);
            let col = db.collection(&col_name).await.expect("collection");

            for i in 0..ops_per_task {
                let id = format!("doc-{}", i);
                let vec = vec![i as f32, (i + 1) as f32, (i + 2) as f32, (i + 3) as f32];

                // 1. Insert/Upsert
                col.upsert(&id, &vec, Some(json!({"task": t, "op": i})))
                    .await
                    .expect("upsert");

                // 2. Immediate retrieval
                let doc = col.get(&id).await.expect("get").expect("found");
                assert_eq!(doc.id, id);

                // 3. Search
                let results = col.search(&vec, 5).await.expect("search");
                assert!(!results.is_empty());
                assert!(results.iter().any(|r| r.id == id));

                // 4. Delete every 5th
                if i % 5 == 0 {
                    col.delete(&id).await.expect("delete");
                    assert!(col.get(&id).await.expect("get").is_none());
                }
            }
        }));
    }

    for h in handles {
        h.await.expect("task panicked");
    }

    // Consistency check: all collections still exist and are accessible
    let list = db.list_collections().await.expect("list");
    for t in 0..num_tasks {
        let col_name = format!("stress-col-{}", t);
        assert!(list.contains(&col_name));
    }
}
