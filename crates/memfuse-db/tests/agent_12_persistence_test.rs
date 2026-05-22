//! E2E persistence and isolation tests for MemFuse collections.
// ANCHOR:INTEGRATION:E2E-002 STATUS:READY AGENT:12 DATE:2026-05-21

use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use tempfile::TempDir;

#[tokio::test]
async fn test_collection_persistence_and_isolation() {
    let tmp = TempDir::new().expect("failed to create temp dir");
    let db_path = tmp.path().to_path_buf();

    let config = MemFuseConfig {
        dimension: 4,
        distance_metric: DistanceMetric::Euclidean,
        ..Default::default()
    };

    // 1. Create data in two collections
    {
        let db = MemFuse::open_with_config(&db_path, config.clone())
            .await
            .expect("open db");

        let col_alpha = db.collection("alpha").await.expect("alpha");
        let col_beta = db.collection("beta").await.expect("beta");

        // Insert same ID in both collections with different values
        col_alpha.insert("shared-id", &[1.0, 0.0, 0.0, 0.0], Some(json!({"source": "alpha"})))
            .await.expect("insert alpha");
        col_beta.insert("shared-id", &[0.0, 1.0, 0.0, 0.0], Some(json!({"source": "beta"})))
            .await.expect("insert beta");

        // Relationships
        col_alpha.insert("other-doc", &[1.0, 1.0, 0.0, 0.0], None).await.expect("ins");
        col_alpha.relate("shared-id", "other-doc", "link").await.expect("relate");

        // Wait for potential background flushes (though LSM should handle it)
    }

    // 2. Re-open and verify persistence
    {
        let db = MemFuse::open_with_config(&db_path, config.clone())
            .await
            .expect("open db re-open");

        // Check collection listing
        let cols = db.list_collections().await.expect("list");
        assert!(cols.contains(&"alpha".to_string()));
        assert!(cols.contains(&"beta".to_string()));

        let col_alpha = db.collection("alpha").await.expect("alpha");
        let col_beta = db.collection("beta").await.expect("beta");

        // Verify isolation of data
        let doc_a = col_alpha.get("shared-id").await.expect("get a").expect("exists");
        let doc_b = col_beta.get("shared-id").await.expect("get b").expect("exists");

        assert_eq!(doc_a.metadata.unwrap()["source"], "alpha");
        assert_eq!(doc_b.metadata.unwrap()["source"], "beta");

        // Verify relationship persistence
        let rels = col_alpha.scan_prefix("__rel:shared-id:link:").await.expect("scan");
        assert_eq!(rels.len(), 1);
        assert_eq!(rels[0].1["to"], "other-doc");

        let rels_beta = col_beta.scan_prefix("__rel:shared-id:link:").await.expect("scan");
        assert_eq!(rels_beta.len(), 0, "Relationships should not leak between collections");

        // Verify vector index persistence (re-loaded on open)
        let results_a = col_alpha.search(&[1.0, 0.0, 0.0, 0.0], 1).await.expect("search a");
        assert_eq!(results_a[0].id, "shared-id");

        // 3. Drop collection
        db.drop_collection("beta").await.expect("drop beta");

        let cols_after = db.list_collections().await.expect("list after");
        assert!(!cols_after.contains(&"beta".to_string()));
    }

    // 4. Final verify after drop
    {
        let db = MemFuse::open_with_config(&db_path, config.clone())
            .await
            .expect("open db final");

        let cols = db.list_collections().await.expect("list final");
        assert!(cols.contains(&"alpha".to_string()));
        assert!(!cols.contains(&"beta".to_string()));

        let col_alpha = db.collection("alpha").await.expect("alpha");
        assert_eq!(col_alpha.len().await, 2);
    }
}
