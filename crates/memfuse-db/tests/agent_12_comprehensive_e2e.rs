//! Comprehensive End-to-End integration test for the MemFuse database.
// ANCHOR:INTEGRATION:E2E-002 STATUS:READY AGENT:12 DATE:2026-05-24

use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use tempfile::TempDir;

#[tokio::test]
async fn test_agent_12_comprehensive_lifecycle_and_persistence() {
    let tmp = TempDir::new().expect("Failed to create temp dir");
    let db_path = tmp.path().to_path_buf();

    let config = MemFuseConfig {
        dimension: 3,
        max_elements: 1000,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };

    // 1. Initial Setup and Insertion
    {
        let db = MemFuse::open_with_config(&db_path, config.clone())
            .await
            .expect("Failed to open DB");

        let col = db.collection("test-col").await.expect("Failed to get collection");

        // Insert docs
        col.insert("doc1", &[1.0, 0.0, 0.0], Some(json!({"text": "Rust is fast", "category": "tech"})))
            .await.expect("Insert doc1");
        col.insert("doc2", &[0.0, 1.0, 0.0], Some(json!({"text": "Python is flexible", "category": "tech"})))
            .await.expect("Insert doc2");
        col.insert("doc3", &[0.0, 0.0, 1.0], Some(json!({"text": "Cooking is an art", "category": "hobby"})))
            .await.expect("Insert doc3");

        // Relate
        col.relate("doc1", "doc2", "friend").await.expect("Relate docs");

        db.close().await.expect("Failed to close DB");
    }

    // 2. Persistence Verification
    {
        let db = MemFuse::open_with_config(&db_path, config.clone())
            .await
            .expect("Failed to re-open DB");

        let col = db.collection("test-col").await.expect("Failed to get collection");

        // Verify docs exist
        assert_eq!(col.len().await, 3);
        let doc1 = col.get("doc1").await.expect("get").expect("exists");
        assert_eq!(doc1.metadata.unwrap()["category"], "tech");

        // Verify relationship
        let relations = col.scan_prefix("__rel:doc1:friend:").await.expect("scan");
        assert_eq!(relations.len(), 1);

        // Hybrid search
        // "fast" should match doc1
        let results = col.hybrid_search("fast", &[1.0, 0.0, 0.0], 2).await.expect("hybrid search");
        assert!(!results.is_empty());
        assert_eq!(results[0].id, "doc1");

        // 3. Update and Partial Verification
        col.update("doc1", &[1.0, 0.0, 0.0], Some(json!({"text": "Rust is very fast", "category": "performance"})))
            .await.expect("Update doc1");

        let doc1_updated = col.get("doc1").await.expect("get").expect("exists");
        assert_eq!(doc1_updated.metadata.as_ref().unwrap()["category"], "performance");

        // Hybrid search with updated text
        let results_updated = col.hybrid_search("performance", &[1.0, 0.0, 0.0], 1).await.expect("hybrid search");
        assert_eq!(results_updated[0].id, "doc1");

        // 4. Collection Isolation
        let col_other = db.collection("other-col").await.expect("other collection");
        assert_eq!(col_other.len().await, 0);

        db.close().await.expect("Failed to close DB second time");
    }

    // 3. Final Persistence Check
    {
        let db = MemFuse::open_with_config(&db_path, config)
            .await
            .expect("Failed to re-open DB third time");
        let col = db.collection("test-col").await.expect("Failed to get collection");

        // Check update persisted
        let doc1 = col.get("doc1").await.expect("get").expect("exists");
        assert_eq!(doc1.metadata.unwrap()["category"], "performance");

        // Delete
        col.delete("doc1").await.expect("delete");
        assert_eq!(col.len().await, 2);
        assert!(col.get("doc1").await.expect("get").is_none());
    }
}
