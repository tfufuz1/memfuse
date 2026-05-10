use memfuse_db::{json, MemFuse, MemFuseConfig};
use tempfile::TempDir;

// ANCHOR:TEST:LAYER-002 — DAG Integrationstest fehlt
// ZIEL: memfuse-db -> memfuse-store (Collection-Persist + Reload)
#[tokio::test]
async fn test_layer_002_collection_persistence() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().to_path_buf();
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };

    {
        let db = MemFuse::open_with_config(&path, config.clone())
            .await
            .unwrap();
        let col = db.collection("persisted_col").await.unwrap();
        col.insert("doc1", &[1.0, 0.0, 0.0, 0.0], Some(json!({"val": 42})))
            .await
            .unwrap();

        let list = db.list_collections().await.unwrap();
        assert!(list.contains(&"persisted_col".to_string()));
    }

    // Re-open
    {
        let db = MemFuse::open_with_config(&path, config).await.unwrap();
        let list = db.list_collections().await.unwrap();
        assert!(
            list.contains(&"persisted_col".to_string()),
            "Collection should be reloaded from storage"
        );

        let col = db.collection("persisted_col").await.unwrap();
        let doc = col
            .get("doc1")
            .await
            .unwrap()
            .expect("Document should persist");
        assert_eq!(doc.id, "doc1");
        assert_eq!(doc.metadata.unwrap()["val"], 42);

        // Verify index is also reloaded
        let results = col.search(&[1.0, 0.0, 0.0, 0.0], 1).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "doc1");
    }
}

// ANCHOR:TEST:LAYER-003 — DAG Integrationstest fehlt
// ZIEL: memfuse-db -> memfuse-text (BM25-Query nach Ingest)
#[tokio::test]
async fn test_layer_003_hybrid_search_text_integration() {
    let tmp = TempDir::new().unwrap();
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(tmp.path(), config).await.unwrap();
    let col = db.collection("text_col").await.unwrap();

    col.insert(
        "doc1",
        &[1.0, 0.0, 0.0, 0.0],
        Some(json!({"text": "the quick brown fox"})),
    )
    .await
    .unwrap();

    col.insert(
        "doc2",
        &[0.0, 1.0, 0.0, 0.0],
        Some(json!({"text": "lazy dog jumps over"})),
    )
    .await
    .unwrap();

    // Text search only (zero vector)
    let results = col
        .hybrid_search("fox", &[0.0, 0.0, 0.0, 0.0], 10)
        .await
        .unwrap();
    assert!(!results.is_empty());
    assert_eq!(results[0].id, "doc1");

    // Text search for the other doc
    let results = col
        .hybrid_search("dog", &[0.0, 0.0, 0.0, 0.0], 10)
        .await
        .unwrap();
    assert!(!results.is_empty());
    assert_eq!(results[0].id, "doc2");

    // Hybrid search
    // doc1 matches vector but doc2 matches text "dog"
    let results = col
        .hybrid_search("dog", &[1.0, 0.0, 0.0, 0.0], 10)
        .await
        .unwrap();
    assert_eq!(results.len(), 2);
    // Both should be present, order depends on RRF
}
