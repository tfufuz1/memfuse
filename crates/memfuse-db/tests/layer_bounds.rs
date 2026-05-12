// ANCHOR:TEST:LAYER-002 — DAG Integrationstest fehlt
// WP:NONE PRIO:3 NEEDS:NONE
// AGENT:04 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-09 DEADLINE:NONE
// ZIEL: memfuse-db -> memfuse-store (Collection-Persist + Reload)

// ANCHOR:TEST:LAYER-003 — DAG Integrationstest fehlt
// WP:NONE PRIO:3 NEEDS:NONE
// AGENT:04 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-09 DEADLINE:NONE
// ZIEL: memfuse-db -> memfuse-text (BM25-Query nach Ingest)

use memfuse_db::{MemFuse, MemFuseConfig};
use tempfile::TempDir;

#[tokio::test]
async fn test_layer_002_collection_persistence() {
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
        let col = db.collection("persistent_col").await.expect("collection");
        col.insert(
            "k1",
            &[1.0, 0.0, 0.0, 0.0],
            Some(serde_json::json!({"val": 42})),
        )
        .await
        .expect("insert");
    }

    // Re-open and verify
    {
        let db = MemFuse::open_with_config(&path, config)
            .await
            .expect("re-open");
        let cols = db.list_collections().await.expect("list");
        assert!(cols.contains(&"persistent_col".to_string()));

        let col = db.collection("persistent_col").await.expect("collection");
        let doc = col.get("k1").await.expect("get").expect("exists");
        assert_eq!(doc.metadata.unwrap()["val"], 42);

        let search = col.search(&[1.0, 0.0, 0.0, 0.0], 1).await.expect("search");
        assert_eq!(search.len(), 1);
        assert_eq!(search[0].id, "k1");
    }
}

#[tokio::test]
async fn test_layer_003_hybrid_search() {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(tmp.path(), config)
        .await
        .expect("open");

    db.insert(
        "doc1",
        &[1.0, 0.0, 0.0, 0.0],
        Some(serde_json::json!({"text": "rust is great"})),
    )
    .await
    .expect("ins1");
    db.insert(
        "doc2",
        &[0.0, 1.0, 0.0, 0.0],
        Some(serde_json::json!({"text": "python is okay"})),
    )
    .await
    .expect("ins2");
    db.insert(
        "doc3",
        &[0.9, 0.1, 0.0, 0.0],
        Some(serde_json::json!({"text": "c++ is fast"})),
    )
    .await
    .expect("ins3");

    // Hybrid search: "rust" + vector close to doc1
    let results = db
        .hybrid_search("rust", &[1.0, 0.0, 0.0, 0.0], 2)
        .await
        .expect("hybrid search");

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].id, "doc1"); // High vector AND text match
}
