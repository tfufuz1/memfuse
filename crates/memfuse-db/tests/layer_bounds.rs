// ANCHOR:TEST:LAYER-002 — DAG Integrationstest fehlt
// WP:NONE PRIO:3 NEEDS:NONE
// AGENT:04 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-09 DEADLINE:NONE
// ZIEL: memfuse-db -> memfuse-store (Collection-Persist + Reload)
#[tokio::test]
async fn test_layer_db_to_store_collection_persistence() {
    use memfuse_db::{MemFuse, MemFuseConfig};
    use serde_json::json;
    use tempfile::TempDir;

    let tmp = TempDir::new().expect("temp dir");
    let path = tmp.path().to_owned();

    {
        let config = MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };
        let db = MemFuse::open_with_config(&path, config)
            .await
            .expect("open db");
        let col = db.collection("persistent_col").await.expect("col");

        col.insert("k1", &[1.0, 0.0, 0.0, 0.0], Some(json!({"val": "find-me"})))
            .await
            .expect("insert");
    }

    // Drop DB and reopen
    {
        let config = MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };
        let db = MemFuse::open_with_config(&path, config)
            .await
            .expect("reopen db");

        let collections = db.list_collections().await.expect("list");
        assert!(
            collections.contains(&"persistent_col".to_string()),
            "Collection should be persisted"
        );

        let col = db.collection("persistent_col").await.expect("get col");
        let doc = col.get("k1").await.expect("get").expect("should exist");
        assert_eq!(doc.metadata.expect("meta")["val"], "find-me");

        let search = col.search(&[1.0, 0.0, 0.0, 0.0], 1).await.expect("search");
        assert_eq!(search[0].id, "k1");
    }
}

// ANCHOR:TEST:LAYER-003 — DAG Integrationstest fehlt
// WP:NONE PRIO:3 NEEDS:NONE
// AGENT:04 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-09 DEADLINE:NONE
// ZIEL: memfuse-db -> memfuse-text (BM25-Query nach Ingest)
#[tokio::test]
async fn test_layer_db_to_text_hybrid_search() {
    use memfuse_db::{MemFuse, MemFuseConfig};
    use serde_json::json;
    use tempfile::TempDir;

    let tmp = TempDir::new().expect("temp dir");
    let db = MemFuse::open_with_config(
        tmp.path(),
        MemFuseConfig {
            dimension: 4,
            ..Default::default()
        },
    )
    .await
    .expect("open db");

    // 1. Insert documents with specific text in metadata
    db.insert(
        "doc-rust",
        &[1.0, 0.0, 0.0, 0.0],
        Some(json!({"text": "The Rust programming language is fast and safe."})),
    )
    .await
    .expect("insert");

    db.insert(
        "doc-python",
        &[0.0, 1.0, 0.0, 0.0],
        Some(json!({"text": "Python is a versatile programming language."})),
    )
    .await
    .expect("insert");

    // 2. Hybrid Search - Text only signal
    let results = db
        .collection("default")
        .await
        .expect("col")
        .hybrid_search("Rust", &[0.0, 0.0, 0.0, 0.0], 1)
        .await
        .expect("hybrid search");

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "doc-rust");

    // 3. Hybrid Search - Combined signals
    // Query vector is close to doc-python, but text query is "Rust"
    let results = db
        .collection("default")
        .await
        .expect("col")
        .hybrid_search("Rust", &[0.0, 0.9, 0.0, 0.0], 2)
        .await
        .expect("hybrid search");

    assert_eq!(results.len(), 2);
    // Both should be present, ranking depends on RRF
    let ids: Vec<String> = results.iter().map(|r| r.id.clone()).collect();
    assert!(ids.contains(&"doc-rust".to_string()));
    assert!(ids.contains(&"doc-python".to_string()));
}
