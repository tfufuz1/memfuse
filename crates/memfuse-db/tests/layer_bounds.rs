use memfuse_db::{MemFuse, MemFuseConfig};
use serde_json::json;
use tempfile::TempDir;

#[tokio::test]
async fn test_layer_002_collection_persistence_and_reload() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().to_owned();
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };

    {
        let db = MemFuse::open_with_config(&path, config.clone())
            .await
            .expect("open db");
        let col = db.collection("persistent_col").await.expect("collection");
        col.insert("doc-1", &[1.0, 0.0, 0.0, 0.0], Some(json!({"val": 1})))
            .await
            .expect("insert");

        assert_eq!(col.len().await, 1);
        // Explicitly drop db to ensure flush (though LsmStorage handles it on drop)
        drop(db);
    }

    // Reopen
    {
        let db = MemFuse::open_with_config(&path, config)
            .await
            .expect("reopen db");

        let collections = db.list_collections().await.expect("list collections");
        assert!(collections.contains(&"persistent_col".to_string()));

        let col = db.collection("persistent_col").await.expect("get collection");
        assert_eq!(col.len().await, 1);

        let doc = col.get("doc-1").await.expect("get doc").expect("exists");
        assert_eq!(doc.id, "doc-1");
        assert_eq!(doc.metadata.unwrap()["val"], 1);

        // Search should also work
        let results = col.search(&[1.0, 0.0, 0.0, 0.0], 1).await.expect("search");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "doc-1");
    }
}

#[tokio::test]
async fn test_layer_003_hybrid_search_bm25() {
    let tmp = TempDir::new().unwrap();
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(tmp.path(), config).await.expect("open db");
    let col = db.collection("hybrid_col").await.expect("collection");

    // Insert documents with text metadata
    col.insert(
        "rust-doc",
        &[1.0, 0.0, 0.0, 0.0],
        Some(json!({"text": "Rust is a systems programming language focusing on safety."})),
    )
    .await
    .expect("insert");

    col.insert(
        "python-doc",
        &[0.0, 1.0, 0.0, 0.0],
        Some(json!({"text": "Python is a high-level programming language."})),
    )
    .await
    .expect("insert");

    // 1. Pure Vector Search
    let vec_results = col.search(&[1.0, 0.0, 0.0, 0.0], 1).await.expect("vector search");
    assert_eq!(vec_results[0].id, "rust-doc");

    // 2. Hybrid Search - favoring text "Python"
    // Use a zero vector to focus on text first to verify text index works
    let hybrid_results = col.hybrid_search("Python", &[0.0, 0.0, 0.0, 0.0], 1).await.expect("hybrid search");
    assert_eq!(hybrid_results.len(), 1);
    assert_eq!(hybrid_results[0].id, "python-doc");

    // 3. Hybrid Search - both vector and text
    // Vector points to rust-doc, but text is "Python"
    let hybrid_results_mixed = col.hybrid_search("Python", &[1.0, 0.0, 0.0, 0.0], 2).await.expect("mixed hybrid search");
    assert_eq!(hybrid_results_mixed.len(), 2);
    // RRF will combine them. Since each is top in its category, they'll both be there.
}
