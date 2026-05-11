//! Layer Boundary Integration Tests for memfuse-db.

use memfuse_db::{MemFuse, MemFuseConfig};
use serde_json::json;
use tempfile::TempDir;

// ANCHOR:TEST:LAYER-002 — DAG Integrationstest implementiert
// AGENT:12 DATE:2026-05-09 STATUS:DONE
// ZIEL: memfuse-db -> memfuse-store (Collection-Persist + Reload)
#[tokio::test]
async fn test_collection_persistence_and_reload() {
    let tmp = TempDir::new().expect("create temp dir");
    let path = tmp.path().to_owned();

    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };

    // 1. Initial Setup and Insertion
    {
        let db = MemFuse::open_with_config(&path, config.clone())
            .await
            .expect("open db");
        let col1 = db.collection("col1").await.expect("create col1");
        let col2 = db.collection("col2").await.expect("create col2");

        col1.insert("doc1", &[1.0, 0.0, 0.0, 0.0], Some(json!({"val": "a"})))
            .await
            .expect("insert col1");
        col2.insert("doc2", &[0.0, 1.0, 0.0, 0.0], Some(json!({"val": "b"})))
            .await
            .expect("insert col2");

        // Wait for potential async flush if any, though MemFuse::open should handle it
    } // db dropped here

    // 2. Reload and Verify
    {
        let db = MemFuse::open_with_config(&path, config)
            .await
            .expect("reopen db");
        let collections = db.list_collections().await.expect("list collections");

        assert!(
            collections.contains(&"col1".to_string()),
            "col1 missing after reload. Collections: {:?}",
            collections
        );
        assert!(
            collections.contains(&"col2".to_string()),
            "col2 missing after reload. Collections: {:?}",
            collections
        );
        assert!(
            collections.contains(&"default".to_string()),
            "default missing after reload"
        );

        let col1 = db.collection("col1").await.expect("get col1");
        let doc1 = col1.get("doc1").await.expect("get doc1").expect("doc1 exists");
        assert_eq!(doc1.metadata.expect("has metadata")["val"], "a");

        let col2 = db.collection("col2").await.expect("get col2");
        let doc2 = col2.get("doc2").await.expect("get doc2").expect("doc2 exists");
        assert_eq!(doc2.metadata.expect("has metadata")["val"], "b");

        // Verify isolation is still maintained
        assert!(col1.get("doc2").await.expect("get").is_none());
        assert!(col2.get("doc1").await.expect("get").is_none());
    }
}

// ANCHOR:TEST:LAYER-003 — DAG Integrationstest implementiert
// AGENT:12 DATE:2026-05-09 STATUS:DONE
// ZIEL: memfuse-db -> memfuse-text (BM25-Query nach Ingest)
#[tokio::test]
async fn test_hybrid_search_bm25_rrf() {
    let tmp = TempDir::new().expect("create temp dir");
    let config = MemFuseConfig {
        dimension: 2,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(tmp.path(), config)
        .await
        .expect("open db");
    let col = db.collection("hybrid").await.expect("create collection");

    // 1. Insert documents with text content and embeddings
    // doc1: strong vector match for [1,0], strong text match for "rust"
    col.insert(
        "doc1",
        &[1.0, 0.0],
        Some(json!({"text": "rust programming language", "desc": "primary"})),
    )
    .await
    .expect("ins1");

    // doc2: strong vector match for [0,1], text match for "python"
    col.insert(
        "doc2",
        &[0.0, 1.0],
        Some(json!({"content": "python scripting language", "desc": "secondary"})),
    )
    .await
    .expect("ins2");

    // doc3: medium vector match for both [0.5, 0.5], text match for both "rust" and "python"
    col.insert(
        "doc3",
        &[0.5, 0.5],
        Some(json!({"text": "rust and python", "desc": "mixed"})),
    )
    .await
    .expect("ins3");

    // 2. Perform Hybrid Search
    // Querying for "rust" and vector [1.0, 0.0]
    let results = col
        .hybrid_search("rust", &[1.0, 0.0], 3)
        .await
        .expect("hybrid search");

    assert_eq!(results.len(), 3, "Should return all 3 docs");

    // doc1 should be first because it matches both best
    assert_eq!(results[0].id, "doc1");

    // doc3 should likely be second because it matches "rust" text and has some vector similarity
    assert_eq!(results[1].id, "doc3");

    // doc2 should be last as it has NO "rust" text and 0 vector similarity to [1,0]
    assert_eq!(results[2].id, "doc2");

    // 3. Test text-only hybrid search (zero vector)
    let text_results = col
        .hybrid_search("python", &[0.0, 0.0], 3)
        .await
        .expect("text hybrid search");
    // Should match doc2 and doc3 which contain "python"
    assert!(text_results.iter().any(|r| r.id == "doc2"));
    assert!(text_results.iter().any(|r| r.id == "doc3"));
    assert!(!text_results.iter().any(|r| r.id == "doc1"));

    // 4. Test vector-only hybrid search (empty text)
    let vec_results = col
        .hybrid_search("", &[0.0, 1.0], 3)
        .await
        .expect("vec hybrid search");
    assert_eq!(vec_results[0].id, "doc2");
}
