//! Stress tests for multi-collection isolation and concurrency.
// AGENT:12 DATE:2026-05-18 STATUS:READY

use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test(flavor = "multi_thread")]
async fn test_multi_collection_stress() -> memfuse_core::Result<()> {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 8,
        max_elements: 1000,
        distance_metric: DistanceMetric::Cosine,
    };
    let db = Arc::new(MemFuse::open_with_config(tmp.path(), config).await?);

    let num_collections = 5;
    let ops_per_collection = 20;
    let mut handles = Vec::new();

    for i in 0..num_collections {
        let db = db.clone();
        let col_name = format!("stress-col-{}", i);

        handles.push(tokio::spawn(async move {
            let col = db.collection(&col_name).await.expect("collection");

            for j in 0..ops_per_collection {
                let id = format!("doc-{}", j);
                // Very distinct vector direction per doc
                let mut vec = vec![0.0f32; 8];
                let angle = (i * ops_per_collection + j) as f32;
                vec[0] = angle.cos();
                vec[1] = angle.sin();

                // Insert
                col.insert(&id, &vec, Some(json!({"col": i, "idx": j})))
                    .await
                    .expect("insert");

                // Search
                let res = col.search(&vec, 1).await.expect("search");
                assert!(!res.is_empty());
                assert_eq!(res[0].id, id);

                // Verify Isolation: should NOT see docs from other collections
                // We test this by searching in the 'default' collection for this vector
                let default_col = db.collection("default").await.expect("default");
                let res_default = default_col.search(&vec, 10).await.expect("search default");
                for r in res_default {
                    assert_ne!(
                        r.id, id,
                        "Found doc from collection {} in default collection",
                        col_name
                    );
                }
            }
        }));
    }

    for h in handles {
        h.await.expect("task failed");
    }

    Ok(())
}
