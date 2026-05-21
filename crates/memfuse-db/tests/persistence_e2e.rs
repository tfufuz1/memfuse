//! End-to-End persistence tests for MemFuse.
// ANCHOR:INTEGRATION:PERSISTENCE-001 STATUS:READY AGENT:12 DATE:2026-05-21

use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use tempfile::TempDir;

#[tokio::test]
async fn test_persistence_across_restarts() {
    let tmp = TempDir::new().expect("Failed to create temp dir");
    let db_path = tmp.path().to_path_buf();

    let config = MemFuseConfig {
        dimension: 3,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };

    // 1. First session: Create data
    {
        let db = MemFuse::open_with_config(&db_path, config.clone())
            .await
            .expect("Failed to open DB");

        let col1 = db.collection("col-1").await.expect("Failed to get col-1");
        col1.insert(
            "doc-1",
            &[1.0, 0.0, 0.0],
            Some(json!({"text": "Persistent data in col 1"})),
        )
        .await
        .expect("Insert failed");

        let col2 = db.collection("col-2").await.expect("Failed to get col-2");
        col2.insert(
            "doc-2",
            &[0.0, 1.0, 0.0],
            Some(json!({"text": "Persistent data in col 2"})),
        )
        .await
        .expect("Insert failed");

        // Force flush to ensure data is moved to SSTables as per memory recommendation
        db.inner_storage()
            .force_flush()
            .await
            .expect("Flush failed");

        // Database is dropped here
    }

    // 2. Second session: Verify data
    {
        let db = MemFuse::open_with_config(&db_path, config.clone())
            .await
            .expect("Failed to re-open DB");

        // Verify collections are listed
        let collections = db
            .list_collections()
            .await
            .expect("List collections failed");
        assert!(collections.contains(&"col-1".to_string()));
        assert!(collections.contains(&"col-2".to_string()));

        // Verify data in col-1
        let col1 = db.collection("col-1").await.expect("Failed to get col-1");
        let doc1 = col1
            .get("doc-1")
            .await
            .expect("Get failed")
            .expect("Doc 1 missing");
        assert_eq!(doc1.metadata.unwrap()["text"], "Persistent data in col 1");

        // Verify search in col-1 (HNSW rebuilt)
        let results = col1
            .search(&[1.0, 0.1, 0.0], 1)
            .await
            .expect("Search failed");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "doc-1");

        // Verify data in col-2
        let col2 = db.collection("col-2").await.expect("Failed to get col-2");
        let doc2 = col2
            .get("doc-2")
            .await
            .expect("Get failed")
            .expect("Doc 2 missing");
        assert_eq!(doc2.metadata.unwrap()["text"], "Persistent data in col 2");
    }
}
