//! Comprehensive E2E tests for MemFuse database.
// AGENT:12 DATE:2026-05-15 STATUS:READY

use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use tempfile::TempDir;

#[tokio::test]
async fn test_full_stack_e2e_workflow() {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        max_elements: 1000,
        distance_metric: DistanceMetric::Cosine,
    };

    // 1. MemFuse::open()
    let db = MemFuse::open_with_config(tmp.path(), config).await.expect("open db");

    // 2. Insert documents with embeddings + metadata
    db.insert(
        "doc-1",
        &[1.0, 0.0, 0.0, 0.0],
        Some(json!({"text": "the quick brown fox", "category": "nature"})),
    ).await.expect("insert 1");

    db.insert(
        "doc-2",
        &[0.0, 1.0, 0.0, 0.0],
        Some(json!({"text": "jumped over the lazy dog", "category": "nature"})),
    ).await.expect("insert 2");

    db.insert(
        "doc-3",
        &[0.0, 0.0, 1.0, 0.0],
        Some(json!({"text": "rust is a systems programming language", "category": "tech"})),
    ).await.expect("insert 3");

    // 3. Hybrid Search (Vector + Text)
    // Querying for "fox" with a vector close to doc-1
    let results = db.hybrid_search("fox", &[1.0, 0.1, 0.0, 0.0], 5).await.expect("hybrid search");

    // 4. Verify Ergebnisse (Score, Metadata, Ordering)
    assert!(!results.is_empty());
    assert_eq!(results[0].id, "doc-1");
    assert!(results[0].score > 0.0);
    assert_eq!(results[0].metadata.as_ref().unwrap()["category"], "nature");

    // 5. Update + Re-Search
    db.update(
        "doc-1",
        &[0.0, 0.0, 0.0, 1.0],
        Some(json!({"text": "updated text", "category": "misc"})),
    ).await.expect("update");

    let results_after_update = db.search(&[0.0, 0.0, 0.0, 1.0], 1).await.expect("search after update");
    assert_eq!(results_after_update[0].id, "doc-1");
    assert_eq!(results_after_update[0].metadata.as_ref().unwrap()["category"], "misc");

    // 6. Delete + Verify Gone
    db.delete("doc-1").await.expect("delete");
    let doc_gone = db.get("doc-1").await.expect("get doc-1");
    assert!(doc_gone.is_none());

    // 7. Collection Isolation
    let col_a = db.collection("col-a").await.expect("col a");
    let col_b = db.collection("col-b").await.expect("col b");

    col_a.insert("iso-1", &[1.0, 0.0, 0.0, 0.0], Some(json!({"val": "a"}))).await.expect("ins a");
    col_b.insert("iso-1", &[1.0, 0.0, 0.0, 0.0], Some(json!({"val": "b"}))).await.expect("ins b");

    let res_a = col_a.get("iso-1").await.expect("get a").unwrap();
    let res_b = col_b.get("iso-1").await.expect("get b").unwrap();

    assert_eq!(res_a.metadata.unwrap()["val"], "a");
    assert_eq!(res_b.metadata.unwrap()["val"], "b");
}
