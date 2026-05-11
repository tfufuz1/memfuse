use memfuse_db::{MemFuse, MemFuseConfig};
use tempfile::TempDir;

// ANCHOR:TEST:LAYER-002 — DAG Integrationstest
// ZIEL: memfuse-db -> memfuse-store (Collection-Persist + Reload)
// AGENT:12 DATE:2026-05-09 STATUS:DONE
#[tokio::test]
async fn test_layer_002_collection_persistence_and_reload() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().to_path_buf();

    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };

    {
        let db = MemFuse::open_with_config(&path, config.clone()).await.unwrap();
        let col = db.collection("persistent_col").await.unwrap();
        col.insert("doc1", &[1.0, 0.0, 0.0, 0.0], Some(serde_json::json!({"text": "hello"}))).await.unwrap();

        let list = db.list_collections().await.unwrap();
        assert!(list.contains(&"persistent_col".to_string()));
    }

    // Reload
    {
        let db = MemFuse::open_with_config(&path, config).await.unwrap();
        let list = db.list_collections().await.unwrap();
        assert!(list.contains(&"persistent_col".to_string()), "Collection should be reloaded from storage");

        let col = db.collection("persistent_col").await.unwrap();
        let doc = col.get("doc1").await.unwrap().expect("Document should be there");
        assert_eq!(doc.id, "doc1");

        let results = col.search(&[1.0, 0.0, 0.0, 0.0], 1).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "doc1");
    }
}

// ANCHOR:TEST:LAYER-003 — DAG Integrationstest
// ZIEL: memfuse-db -> memfuse-text (BM25-Query nach Ingest)
// AGENT:12 DATE:2026-05-09 STATUS:DONE
#[tokio::test]
async fn test_layer_003_bm25_query_after_ingest() {
    let tmp = TempDir::new().unwrap();
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(tmp.path(), config).await.unwrap();
    let col = db.collection("text_col").await.unwrap();

    col.insert("doc1", &[0.1; 4], Some(serde_json::json!({"text": "The quick brown fox"}))).await.unwrap();
    col.insert("doc2", &[0.2; 4], Some(serde_json::json!({"text": "Jumps over the lazy dog"}))).await.unwrap();

    // Hybrid search with vector = 0 means pure text search
    let results = col.hybrid_search("fox", &[0.0; 4], 10).await.unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "doc1");

    let results2 = col.hybrid_search("lazy dog", &[0.0; 4], 10).await.unwrap();
    assert_eq!(results2.len(), 1);
    assert_eq!(results2[0].id, "doc2");
}
