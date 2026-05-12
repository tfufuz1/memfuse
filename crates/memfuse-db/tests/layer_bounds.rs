use memfuse_db::MemFuse;
use tempfile::TempDir;

// ANCHOR:TEST:LAYER-002 — Interaction between memfuse-db and memfuse-store for collection persistence and reload.
#[tokio::test]
async fn test_layer_002_collection_persistence() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().to_owned();

    {
        let db = MemFuse::open(&path).await.unwrap();
        let col = db.collection("persistent_col").await.unwrap();
        col.insert("k1", &[1.0; 1536], Some(serde_json::json!({"v": 1})))
            .await
            .unwrap();
    }

    // Re-open
    {
        let db = MemFuse::open(&path).await.unwrap();
        let cols = db.list_collections().await.unwrap();
        assert!(cols.contains(&"persistent_col".to_string()));

        let col = db.collection("persistent_col").await.unwrap();
        let doc = col.get("k1").await.unwrap().expect("should persist");
        assert_eq!(doc.metadata.unwrap()["v"], 1);
    }
}

// ANCHOR:TEST:LAYER-003 — Interaction between memfuse-db and memfuse-text for BM25 Query after Ingest.
#[tokio::test]
async fn test_layer_003_bm25_integration() {
    let tmp = TempDir::new().unwrap();
    let db = MemFuse::open(tmp.path()).await.unwrap();
    let col = db.collection("text_col").await.unwrap();

    col.insert(
        "doc1",
        &[0.1; 1536],
        Some(serde_json::json!({"text": "rust programming language"})),
    )
    .await
    .unwrap();

    col.insert(
        "doc2",
        &[0.2; 1536],
        Some(serde_json::json!({"text": "python scripting language"})),
    )
    .await
    .unwrap();

    // Hybrid search with text query
    let results = col
        .hybrid_search("rust", &[0.0; 1536], 10)
        .await
        .expect("hybrid search");

    assert!(!results.is_empty());
    assert_eq!(results[0].id, "doc1");
}
