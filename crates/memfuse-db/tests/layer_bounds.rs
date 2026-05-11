// ANCHOR:TEST:LAYER-002 — DAG Integrationstest implementiert.
// WP:NONE PRIO:3 NEEDS:NONE
// AGENT:04 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-09 DEADLINE:NONE
// ZIEL: memfuse-db -> memfuse-store (Collection-Persist + Reload)

// ANCHOR:TEST:LAYER-003 — DAG Integrationstest implementiert.
// WP:NONE PRIO:3 NEEDS:NONE
// AGENT:04 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-09 DEADLINE:NONE
// ZIEL: memfuse-db -> memfuse-text (BM25-Query nach Ingest)

use memfuse_db::{MemFuse, MemFuseConfig};
use serde_json::json;
use tempfile::TempDir;

#[tokio::test]
async fn test_collection_persistence_reload() {
    let tmp = TempDir::new().expect("temp dir");
    let path = tmp.path().to_owned();
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };

    {
        let db = MemFuse::open_with_config(&path, config.clone())
            .await
            .expect("open db");
        let col = db.collection("persistent_col").await.expect("create col");
        col.insert("k1", &[1.0, 0.0, 0.0, 0.0], Some(json!({"val": "p1"})))
            .await
            .expect("insert");
    } // Drop DB

    {
        let db = MemFuse::open_with_config(&path, config)
            .await
            .expect("re-open db");
        let list = db.list_collections().await.expect("list");
        assert!(list.contains(&"persistent_col".to_string()));

        let col = db.collection("persistent_col").await.expect("get col");
        let doc = col.get("k1").await.expect("get").expect("exists");
        assert_eq!(doc.id, "k1");
        assert_eq!(doc.metadata.expect("meta")["val"], "p1");

        // HNSW index should also be reloaded
        let results = col.search(&[1.0, 0.0, 0.0, 0.0], 1).await.expect("search");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "k1");
    }
}

#[tokio::test]
async fn test_hybrid_search_bm25_flow() {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(tmp.path(), config)
        .await
        .expect("open db");

    db.insert(
        "doc-rust",
        &[1.0, 0.0, 0.0, 0.0],
        Some(json!({"text": "Rust is a systems programming language"})),
    )
    .await
    .expect("insert");

    db.insert(
        "doc-python",
        &[0.0, 1.0, 0.0, 0.0],
        Some(json!({"content": "Python is great for data science"})),
    )
    .await
    .expect("insert");

    // BM25 only search (zero vector)
    let zero_vec = vec![0.0; 4];
    let results = db
        .collection("default")
        .await
        .expect("col")
        .hybrid_search("systems programming", &zero_vec, 10)
        .await
        .expect("hybrid search");

    assert!(!results.is_empty());
    assert_eq!(results[0].id, "doc-rust");

    // Hybrid search
    let results = db
        .collection("default")
        .await
        .expect("col")
        .hybrid_search("data science", &[0.0, 0.9, 0.0, 0.0], 10)
        .await
        .expect("hybrid search");

    assert!(!results.is_empty());
    assert_eq!(results[0].id, "doc-python");
}
