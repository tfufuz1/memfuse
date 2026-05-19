//! Persistence E2E tests for MemFuse.
// ANCHOR:INTEGRATION:PERSISTENCE-001 STATUS:READY AGENT:12 DATE:2026-05-19

use memfuse_db::{MemFuse, MemFuseConfig, DistanceMetric};
use serde_json::json;
use tempfile::TempDir;

#[tokio::test]
async fn test_persistence_across_restarts() {
    let tmp = TempDir::new().expect("Failed to create temp dir");
    let path = tmp.path().to_path_buf();

    let config = MemFuseConfig {
        dimension: 3,
        distance_metric: DistanceMetric::Euclidean,
        ..Default::default()
    };

    // Phase 1: Write data
    {
        let db = MemFuse::open_with_config(&path, config.clone())
            .await
            .expect("Failed to open DB");

        let col1 = db.collection("col1").await.expect("Failed to get col1");
        col1.insert("d1", &[1.0, 0.0, 0.0], Some(json!({"val": "one"}))).await.expect("insert");

        let col2 = db.collection("col2").await.expect("Failed to get col2");
        col2.insert("d2", &[0.0, 1.0, 0.0], Some(json!({"val": "two"}))).await.expect("insert");

        db.relate("d1", "d2", "links").await.expect("relate"); // Note: relate uses default_col if not specified, but MemFuse::relate uses default_col.
        // Wait, MemFuse::relate uses default_col. I should probably use col1.relate or check if I want to test default col too.

        let default_col = db.collection("default").await.expect("default col");
        default_col.insert("def1", &[0.0, 0.0, 1.0], Some(json!({"val": "default"}))).await.expect("insert");

        // Let's use relate on col1
        col1.relate("d1", "d1_related", "self").await.expect("relate on col1");

        // Ensure data is flushed (dropping db handles this as it's an embedded DB)
    }

    // Phase 2: Reopen and Verify
    {
        let db = MemFuse::open_with_config(&path, config)
            .await
            .expect("Failed to reopen DB");

        // Check collections exist
        let cols = db.list_collections().await.expect("list collections");
        assert!(cols.contains(&"col1".to_string()));
        assert!(cols.contains(&"col2".to_string()));
        assert!(cols.contains(&"default".to_string()));

        // Verify data in col1
        let col1 = db.collection("col1").await.expect("Failed to get col1");
        let d1 = col1.get("d1").await.expect("get").expect("d1 missing");
        assert_eq!(d1.metadata.unwrap()["val"], "one");

        let search1 = col1.search(&[1.0, 0.0, 0.0], 1).await.expect("search");
        assert_eq!(search1[0].id, "d1");

        // Verify relationship in col1
        let relations = col1.scan_prefix("__rel:d1:self:").await.expect("scan");
        assert_eq!(relations.len(), 1);
        assert_eq!(relations[0].1["to"], "d1_related");

        // Verify data in col2
        let col2 = db.collection("col2").await.expect("Failed to get col2");
        let d2 = col2.get("d2").await.expect("get").expect("d2 missing");
        assert_eq!(d2.metadata.unwrap()["val"], "two");

        // Verify data in default col
        let def_col = db.collection("default").await.expect("default col");
        let def1 = def_col.get("def1").await.expect("get").expect("def1 missing");
        assert_eq!(def1.metadata.unwrap()["val"], "default");
    }
}
