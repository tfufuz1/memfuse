// ANCHOR:TEST:LAYER-002 — DAG Integrationstest fehlt
// WP:NONE PRIO:3 NEEDS:NONE
// AGENT:04 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-09 DEADLINE:NONE
// ZIEL: memfuse-db -> memfuse-store (Collection-Persist + Reload)
// AGENT:antigravity DATE:2026-05-09 STATUS:DONE

use memfuse_db::{MemFuse, MemFuseConfig, DistanceMetric};
use tempfile::TempDir;
use serde_json::json;

#[tokio::test]
async fn test_layer_002_collection_persistence_reload() {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        max_elements: 1000,
        distance_metric: DistanceMetric::Cosine,
    };

    {
        let db = MemFuse::open_with_config(tmp.path(), config.clone())
            .await
            .expect("open db");
        let col = db.collection("persistent_col").await.expect("create col");
        col.insert("k1", &[1.0, 0.0, 0.0, 0.0], Some(json!({"val": "persisted"})))
            .await
            .expect("insert");

        let list = db.list_collections().await.expect("list");
        assert!(list.contains(&"persistent_col".to_string()));
    }

    // Reopen and check
    {
        let db = MemFuse::open_with_config(tmp.path(), config)
            .await
            .expect("reopen db");

        let list = db.list_collections().await.expect("list after reload");
        assert!(list.contains(&"persistent_col".to_string()));

        let col = db.collection("persistent_col").await.expect("get col");
        let doc = col.get("k1").await.expect("get doc").expect("exists");
        assert_eq!(doc.metadata.expect("meta")["val"], "persisted");

        assert_eq!(col.len().await, 1);
    }
}

// ANCHOR:TEST:LAYER-003 — DAG Integrationstest fehlt
// WP:NONE PRIO:3 NEEDS:NONE
// AGENT:04 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-09 DEADLINE:NONE
// ZIEL: memfuse-db -> memfuse-text (BM25-Query nach Ingest)
// AGENT:antigravity DATE:2026-05-09 STATUS:DONE

#[tokio::test]
async fn test_layer_003_hybrid_search_bm25() {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        max_elements: 1000,
        distance_metric: DistanceMetric::Cosine,
    };

    let db = MemFuse::open_with_config(tmp.path(), config)
        .await
        .expect("open db");

    db.insert(
        "doc1",
        &[1.0, 0.0, 0.0, 0.0],
        Some(json!({"text": "the quick brown fox"})),
    )
    .await
    .expect("insert 1");

    db.insert(
        "doc2",
        &[0.0, 1.0, 0.0, 0.0],
        Some(json!({"text": "jumped over the lazy dog"})),
    )
    .await
    .expect("insert 2");

    // Pure text search via hybrid_search (zero vector)
    let results = db.hybrid_search("fox", &[0.0, 0.0, 0.0, 0.0], 5).await.expect("hybrid search");
    assert!(!results.is_empty());
    assert_eq!(results[0].id, "doc1");

    // Pure vector search via hybrid_search (empty text)
    let results = db.hybrid_search("", &[0.0, 1.0, 0.0, 0.0], 5).await.expect("hybrid search");
    assert!(!results.is_empty());
    assert_eq!(results[0].id, "doc2");

    // Hybrid search
    let results = db.hybrid_search("fox", &[0.0, 1.0, 0.0, 0.0], 5).await.expect("hybrid search");
    assert!(results.len() >= 2);
    // Both should be in results due to RRF
    let ids: Vec<String> = results.iter().map(|r| r.id.clone()).collect();
    assert!(ids.contains(&"doc1".to_string()));
    assert!(ids.contains(&"doc2".to_string()));
}
