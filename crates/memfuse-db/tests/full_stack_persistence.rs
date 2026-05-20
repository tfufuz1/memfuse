//! Full-stack persistence tests for MemFuse.
// ANCHOR:INTEGRATION:PERSISTENCE-001 STATUS:READY AGENT:12 DATE:2026-05-20

use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use tempfile::TempDir;

#[tokio::test]
async fn test_full_stack_persistence_roundtrip() {
    let tmp = TempDir::new().expect("Failed to create temp dir");
    let db_path = tmp.path().to_path_buf();

    let config = MemFuseConfig {
        dimension: 3,
        max_elements: 1000,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };

    // 1. Initial State: Create DB and insert data
    {
        let db = MemFuse::open_with_config(&db_path, config.clone())
            .await
            .expect("Failed to open DB");

        // Insert with Vector + Text
        db.insert(
            "persist-1",
            &[1.0, 0.0, 0.0],
            Some(json!({"text": "Persistent vector and text data", "category": "test"})),
        )
        .await
        .expect("Insert failed");

        // Insert another for relationship
        db.insert(
            "persist-2",
            &[0.0, 1.0, 0.0],
            Some(json!({"text": "Another piece of data"})),
        )
        .await
        .expect("Insert 2 failed");

        // Relate them
        db.relate("persist-1", "persist-2", "related_to")
            .await
            .expect("Relate failed");

        // Verify it works before restart
        let res = db.search(&[1.0, 0.0, 0.0], 1).await.expect("Search failed");
        assert_eq!(res[0].id, "persist-1");

        let hybrid = db
            .hybrid_search("Persistent", &[1.0, 0.0, 0.0], 1)
            .await
            .expect("Hybrid failed");
        assert_eq!(hybrid[0].id, "persist-1");

        let rels = db
            .scan_prefix("__rel:persist-1:")
            .await
            .expect("Scan failed");
        assert_eq!(rels.len(), 1);

        // Drop DB to simulate restart
    }

    // 2. Recovery State: Re-open DB and verify data
    {
        let db = MemFuse::open_with_config(&db_path, config)
            .await
            .expect("Failed to re-open DB");

        // A. Verify Vector Index persistence
        let res = db
            .search(&[1.0, 0.0, 0.0], 1)
            .await
            .expect("Search after restart failed");
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].id, "persist-1");

        // B. Verify Text Index persistence
        let hybrid = db
            .hybrid_search("Persistent", &[1.0, 0.0, 0.0], 1)
            .await
            .expect("Hybrid after restart failed");
        assert_eq!(hybrid.len(), 1);
        assert_eq!(hybrid[0].id, "persist-1");

        // C. Verify Relationships persistence
        let rels = db
            .scan_prefix("__rel:persist-1:")
            .await
            .expect("Scan after restart failed");
        assert_eq!(rels.len(), 1);
        assert_eq!(rels[0].1["to"], "persist-2");

        // D. Verify Metadata integrity
        let doc = db
            .get("persist-1")
            .await
            .expect("Get failed")
            .expect("Not found");
        assert_eq!(doc.metadata.unwrap()["category"], "test");

        // E. Verify Collection list persistence
        let collections = db
            .list_collections()
            .await
            .expect("List collections failed");
        assert!(collections.contains(&"default".to_string()));
    }
}

#[tokio::test]
async fn test_named_collection_persistence() {
    let tmp = TempDir::new().expect("Failed to create temp dir");
    let db_path = tmp.path().to_path_buf();
    let config = MemFuseConfig {
        dimension: 2,
        ..Default::default()
    };

    let col_name = "persistent-collection";

    {
        let db = MemFuse::open_with_config(&db_path, config.clone())
            .await
            .expect("Failed to open DB");
        let col = db
            .collection(col_name)
            .await
            .expect("Failed to create collection");

        col.insert("col-doc-1", &[1.0, 1.0], Some(json!({"source": "col"})))
            .await
            .expect("Insert into collection failed");
    }

    // Re-open
    {
        let db = MemFuse::open_with_config(&db_path, config)
            .await
            .expect("Failed to re-open DB");

        let collections = db
            .list_collections()
            .await
            .expect("List collections failed");
        assert!(collections.contains(&col_name.to_string()));

        let col = db
            .collection(col_name)
            .await
            .expect("Failed to get collection");
        let doc = col
            .get("col-doc-1")
            .await
            .expect("Get failed")
            .expect("Not found");
        assert_eq!(doc.id, "col-doc-1");

        let search = col.search(&[1.0, 1.0], 1).await.expect("Search failed");
        assert_eq!(search[0].id, "col-doc-1");
    }
}
