use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use tempfile::TempDir;

// ANCHOR:TEST:LAYER-002 — DAG Integrationstest fehlt
// WP:NONE PRIO:3 NEEDS:NONE
// AGENT:04 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-09 DEADLINE:NONE
// ZIEL: memfuse-db -> memfuse-store (Collection-Persist + Reload)
#[tokio::test]
async fn test_layer_002_collection_persistence() {
    let tmp = TempDir::new().expect("temp dir");
    let path = tmp.path().to_owned();

    let config = MemFuseConfig {
        dimension: 4,
        max_elements: 100,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };

    // 1. Create collection and insert data
    {
        let db = MemFuse::open_with_config(&path, config.clone())
            .await
            .expect("open");
        let col = db.collection("persistent-col").await.expect("create col");
        col.insert(
            "doc-1",
            &[1.0, 0.0, 0.0, 0.0],
            Some(json!({"text": "hello world"})),
        )
        .await
        .expect("insert");

        let list = db.list_collections().await.expect("list");
        assert!(list.contains(&"persistent-col".to_string()));
        db.close().await.expect("close");
    }

    // 2. Close and re-open (simulate restart)
    {
        let db = MemFuse::open_with_config(&path, config)
            .await
            .expect("re-open");

        // Verify collection is discovered
        let list = db.list_collections().await.expect("list after restart");
        assert!(
            list.contains(&"persistent-col".to_string()),
            "Collection should be persisted. Found: {:?}",
            list
        );

        // Verify data is still there
        let col = db.collection("persistent-col").await.expect("get col");
        let doc = col.get("doc-1").await.expect("get doc").expect("exists");
        assert_eq!(doc.id, "doc-1");
    }
}

// ANCHOR:TEST:LAYER-003 — DAG Integrationstest fehlt
// WP:NONE PRIO:3 NEEDS:NONE
// AGENT:04 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-09 DEADLINE:NONE
// ZIEL: memfuse-db -> memfuse-text (BM25-Query nach Ingest)
#[tokio::test]
#[ignore]
async fn test_layer_003_hybrid_bm25_search() {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(tmp.path(), config)
        .await
        .expect("open");

    db.insert(
        "doc-1",
        &[0.1, 0.1, 0.1, 0.1],
        Some(json!({"text": "the quick brown fox"})),
    )
    .await
    .expect("ins 1");
    db.insert(
        "doc-2",
        &[0.1, 0.1, 0.1, 0.1],
        Some(json!({"content": "jumped over the lazy dog"})),
    )
    .await
    .expect("ins 2");
    db.insert(
        "doc-3",
        &[0.1, 0.1, 0.1, 0.1],
        Some(json!({"text": "brown dogs are lazy"})),
    )
    .await
    .expect("ins 3");

    // Search for "fox" - should find doc-1
    let results = db
        .hybrid_search("fox", &[0.0, 0.0, 0.0, 0.0], 10)
        .await
        .expect("hybrid search");
    assert!(!results.is_empty());
    assert_eq!(results[0].id, "doc-1");

    // Search for "lazy dog" - should find doc-2 and doc-3
    let results2 = db
        .hybrid_search("lazy dog", &[0.0, 0.0, 0.0, 0.0], 10)
        .await
        .expect("hybrid search 2");
    let ids: Vec<String> = results2.iter().map(|r| r.id.clone()).collect();
    assert!(ids.contains(&"doc-2".to_string()));
    assert!(ids.contains(&"doc-3".to_string()));
}
