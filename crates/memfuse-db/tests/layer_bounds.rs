// ANCHOR:TEST:LAYER-002 — DAG Integrationstest
// WP:NONE PRIO:3 NEEDS:NONE
// AGENT:12 DATE:2026-05-10 STATUS:DONE
// CREATED:2026-05-09 DEADLINE:NONE
// ZIEL: memfuse-db -> memfuse-store (Collection-Persist + Reload)

use memfuse_db::{MemFuse, MemFuseConfig};
use tempfile::TempDir;

#[tokio::test]
async fn test_collection_persistence_and_reload() {
    let tmp = TempDir::new().expect("temp dir");
    let path = tmp.path().to_path_buf();

    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };

    {
        let db = MemFuse::open_with_config(&path, config.clone())
            .await
            .expect("open");
        let col = db.collection("persistent_col").await.expect("create col");
        col.insert("k1", &[1.0, 0.0, 0.0, 0.0], None)
            .await
            .expect("insert");
        // DB and Col dropped here
    }

    {
        let db = MemFuse::open_with_config(&path, config)
            .await
            .expect("reopen");
        let collections = db.list_collections().await.expect("list");
        assert!(
            collections.contains(&"persistent_col".to_string()),
            "Collection should be reloaded from LSM"
        );

        let col = db.collection("persistent_col").await.expect("get col");
        assert_eq!(col.len().await, 1);
        let doc = col.get("k1").await.expect("get").expect("exists");
        assert_eq!(doc.id, "k1");
    }
}

// ANCHOR:TEST:LAYER-003 — DAG Integrationstest
// WP:NONE PRIO:3 NEEDS:NONE
// AGENT:12 DATE:2026-05-10 STATUS:DONE
// CREATED:2026-05-09 DEADLINE:NONE
// ZIEL: memfuse-db -> memfuse-text (BM25-Query nach Ingest)

#[tokio::test]
async fn test_db_to_text_integration() {
    let tmp = TempDir::new().expect("temp dir");
    let db = MemFuse::open(tmp.path()).await.expect("open");
    let col = db.collection("text_test").await.expect("col");

    col.insert(
        "doc1",
        &[0.0; 1536],
        Some(serde_json::json!({"text": "rust programming is great"})),
    )
    .await
    .expect("ins1");
    col.insert(
        "doc2",
        &[0.0; 1536],
        Some(serde_json::json!({"text": "python programming is also good"})),
    )
    .await
    .expect("ins2");

    // Hybrid search with zero vector = pure BM25
    let results = col
        .hybrid_search("rust", &[0.0; 1536], 10)
        .await
        .expect("search");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "doc1");

    let results2 = col
        .hybrid_search("programming", &[0.0; 1536], 10)
        .await
        .expect("search2");
    assert_eq!(results2.len(), 2);
}
