//! Stress tests for MemFuse database orchestrator.

use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::task::JoinHandle;

#[tokio::test(flavor = "multi_thread")]
async fn test_orchestrator_stress_concurrency() {
    let tmp = TempDir::new().expect("temp dir"); // expect #[cfg(test)]
    let config = MemFuseConfig {
        dimension: 4,
        max_elements: 10000,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };
    let db = Arc::new(
        MemFuse::open_with_config(tmp.path(), config)
            .await
            .expect("open db"), // expect #[cfg(test)]
    );

    let num_tasks = 10;
    let ops_per_task = 50;
    let mut handles: Vec<JoinHandle<()>> = Vec::new();

    for t in 0..num_tasks {
        let db = db.clone();
        handles.push(tokio::spawn(async move {
            // Give each task its OWN collection to avoid ID/Vector collisions during search tests
            let col_name = format!("col-{}", t);
            let col = db.collection(&col_name).await.expect("collection"); // expect #[cfg(test)]

            for i in 0..ops_per_task {
                let id = format!("doc-{}", i);
                let vec = vec![i as f32, (i + 1) as f32, (i + 2) as f32, (i + 3) as f32];

                // Insert
                col.insert(&id, &vec, Some(json!({"t": t, "i": i})))
                    .await
                    .expect("insert"); // expect #[cfg(test)]

                // Search - should find itself
                let results = col.search(&vec, 1).await.expect("search"); // expect #[cfg(test)]
                assert!(!results.is_empty());
                assert_eq!(
                    results[0].id, id,
                    "Task {} failed to find its own doc {} in {}",
                    t, id, col_name
                );

                // Delete (every 4th)
                if i % 4 == 0 {
                    col.delete(&id).await.expect("delete"); // expect #[cfg(test)]
                    let doc = col.get(&id).await.expect("get"); // expect #[cfg(test)]
                    assert!(doc.is_none());
                }
            }
        }));
    }

    for h in handles {
        h.await.expect("task panicked"); // expect #[cfg(test)]
    }

    // Final sanity check
    let list = db.list_collections().await.expect("list"); // expect #[cfg(test)]
    assert!(list.len() >= num_tasks);
}
