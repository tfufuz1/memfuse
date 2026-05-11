// ANCHOR:TEST:LAYER-002 — Collection-Persist + Reload Integrationstest
// WP:NONE PRIO:3 NEEDS:NONE
// AGENT:04 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-09 DEADLINE:NONE
// ZIEL: memfuse-db -> memfuse-store (Collection-Persist + Reload)
// AGENT:antigravity DATE:2026-05-09 STATUS:DONE

use memfuse_db::{MemFuse, MemFuseConfig};
use tempfile::TempDir;

#[tokio::test]
async fn test_collection_persistence_and_reload() {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };

    {
        let db = MemFuse::open_with_config(tmp.path(), config.clone())
            .await
            .expect("open db");
        let col = db.collection("persistent-col").await.expect("create col");
        col.insert("k1", &[1.0, 0.0, 0.0, 0.0], Some(serde_json::json!({"v": 1})))
            .await
            .expect("insert");

        let list = db.list_collections().await.expect("list");
        assert!(list.contains(&"persistent-col".to_string()));
    }

    // Re-open
    {
        let db = MemFuse::open_with_config(tmp.path(), config)
            .await
            .expect("re-open db");

        let list = db.list_collections().await.expect("list after reload");
        assert!(list.contains(&"persistent-col".to_string()), "Collection should be persisted and reloaded");

        let col = db.collection("persistent-col").await.expect("get col");
        let doc = col.get("k1").await.expect("get k1").expect("should exist");
        assert_eq!(doc.metadata.expect("meta")["v"], 1);

        let results = col.search(&[1.0, 0.0, 0.0, 0.0], 1).await.expect("search");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "k1");
    }
}

// ANCHOR:TEST:LAYER-003 — Hybrid-Search BM25 Integrationstest
// WP:NONE PRIO:3 NEEDS:NONE
// AGENT:04 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-09 DEADLINE:NONE
// ZIEL: memfuse-db -> memfuse-text (BM25-Query nach Ingest)
// AGENT:antigravity DATE:2026-05-09 STATUS:DONE

#[tokio::test]
async fn test_hybrid_search_bm25_integration() {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };

    let db = MemFuse::open_with_config(tmp.path(), config)
        .await
        .expect("open db");

    db.insert("doc-1", &[1.0, 0.0, 0.0, 0.0], Some(serde_json::json!({"text": "the quick brown fox"})))
        .await
        .expect("insert 1");
    db.insert("doc-2", &[0.0, 1.0, 0.0, 0.0], Some(serde_json::json!({"text": "jumped over the lazy dog"})))
        .await
        .expect("insert 2");

    // Test pure BM25 (zero vector)
    let col = db.collection("default").await.expect("default col");
    let results = col.hybrid_search("fox", &[0.0, 0.0, 0.0, 0.0], 10).await.expect("hybrid search");

    assert!(!results.is_empty());
    assert_eq!(results[0].id, "doc-1");

    // Test fusion
    let results_fused = col.hybrid_search("dog", &[1.0, 0.0, 0.0, 0.0], 10).await.expect("hybrid search fused");
    // doc-1 matches vector, doc-2 matches text "dog"
    assert!(results_fused.iter().any(|r| r.id == "doc-1"));
    assert!(results_fused.iter().any(|r| r.id == "doc-2"));
}

#[tokio::test]
async fn test_memfuse_facade_hybrid_search() {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };

    let db = MemFuse::open_with_config(tmp.path(), config)
        .await
        .expect("open db");

    db.insert("rust-doc", &[1.0, 0.0, 0.0, 0.0], Some(serde_json::json!({"text": "rust programming language"})))
        .await
        .expect("insert 1");
    db.insert("python-doc", &[0.0, 1.0, 0.0, 0.0], Some(serde_json::json!({"text": "python programming language"})))
        .await
        .expect("insert 2");

    // Search via facade
    let results = db.hybrid_search("rust", &[0.0, 0.0, 0.0, 0.0], 5).await.expect("facade hybrid search");
    assert!(!results.is_empty());
    assert_eq!(results[0].id, "rust-doc");
}
