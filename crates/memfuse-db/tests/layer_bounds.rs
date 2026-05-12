//! Integration tests for DAG layer boundaries.
//! Ensures that higher layers (DB) interact correctly with lower layers (Store, Text).

use memfuse_db::{MemFuse, MemFuseConfig};
use tempfile::TempDir;

// ANCHOR:TEST:LAYER-002 — DAG Integrationstest
// WP:NONE PRIO:3 NEEDS:NONE
// AGENT:04 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-09 DEADLINE:NONE
// ZIEL: memfuse-db -> memfuse-store (Collection-Persist + Reload)
#[tokio::test]
async fn test_layer_002_persistence_reload() {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };

    {
        let db = MemFuse::open_with_config(tmp.path(), config.clone())
            .await
            .expect("open");
        db.collection("persistent-col")
            .await
            .expect("create col")
            .insert("k1", &[1.0, 0.0, 0.0, 0.0], None)
            .await
            .expect("insert");
    }

    // Reload and check if collection and data still exist
    {
        let db = MemFuse::open_with_config(tmp.path(), config)
            .await
            .expect("open");
        let collections = db.list_collections().await.expect("list");
        assert!(collections.contains(&"persistent-col".to_string()));

        let col = db.collection("persistent-col").await.expect("get col");
        assert_eq!(col.len().await, 1);
        let doc = col.get("k1").await.expect("get").expect("exists");
        assert_eq!(doc.id, "k1");
    }
}

// ANCHOR:TEST:LAYER-003 — DAG Integrationstest
// WP:NONE PRIO:3 NEEDS:NONE
// AGENT:04 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-09 DEADLINE:NONE
// ZIEL: memfuse-db -> memfuse-text (BM25-Query nach Ingest)
#[tokio::test]
async fn test_layer_003_text_search() {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(tmp.path(), config)
        .await
        .expect("open");

    db.insert(
        "doc-text",
        &[0.0, 0.0, 0.0, 0.0],
        Some(serde_json::json!({"text": "the quick brown fox jumps over the lazy dog"})),
    )
    .await
    .expect("insert");

    // Hybrid search with zero vector should rely on BM25
    let results = db
        .hybrid_search("fox dog", &[0.0, 0.0, 0.0, 0.0], 5)
        .await
        .expect("hybrid search");

    assert!(!results.is_empty());
    assert_eq!(results[0].id, "doc-text");
}
