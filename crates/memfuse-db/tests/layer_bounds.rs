//! Layer boundary and persistence integration tests.
// ANCHOR:ARCH:LAYER-TEST-001 — Sicherstellen, dass Layer 2 (Orchestrator) sauber auf Layer 1 (Store/Index) sitzt.
// WP:WP-1.2 PRIO:1 NEEDS:NONE
// AGENT:04 DATE:2026-05-09 STATUS:DONE

use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use tempfile::TempDir;

#[tokio::test]
async fn test_layer_002_collection_persistence() {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        max_elements: 100,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };

    {
        let db = MemFuse::open_with_config(tmp.path(), config.clone())
            .await
            .expect("open db");
        let col = db.collection("persistent").await.expect("collection");
        col.insert("k1", &[1.0, 0.0, 0.0, 0.0], Some(json!({"v": 1})))
            .await
            .expect("insert");
    }

    // Re-open and check persistence
    {
        let db = MemFuse::open_with_config(tmp.path(), config)
            .await
            .expect("open db");
        let col = db.collection("persistent").await.expect("collection");
        assert_eq!(col.len().await, 1);
        let doc = col.get("k1").await.expect("get").expect("exists");
        assert_eq!(doc.metadata.expect("meta")["v"], 1);
    }
}

#[tokio::test]
async fn test_layer_003_hybrid_bm25_search() {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(tmp.path(), config)
        .await
        .expect("open db");
    let col = db.collection("hybrid").await.expect("collection");

    col.insert(
        "doc1",
        &[1.0, 0.0, 0.0, 0.0],
        Some(json!({"text": "Rust is safe"})),
    )
    .await
    .expect("ins");
    col.insert(
        "doc2",
        &[0.0, 1.0, 0.0, 0.0],
        Some(json!({"text": "Python is fast"})),
    )
    .await
    .expect("ins");

    // Hybrid search
    let results = col
        .hybrid_search("Rust", &[1.0, 0.0, 0.0, 0.0], 5)
        .await
        .expect("search");
    assert_eq!(results[0].id, "doc1");
}
