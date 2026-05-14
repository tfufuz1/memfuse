//! Comprehensive E2E Integration Test for MemFuse.

use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use tempfile::TempDir;

#[tokio::test]
async fn test_full_stack_e2e_flow() {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        max_elements: 1000,
        distance_metric: DistanceMetric::Cosine,
    };

    // 1. Open DB
    let db = MemFuse::open_with_config(tmp.path(), config)
        .await
        .expect("open db");

    let col = db.collection("e2e-test").await.expect("collection");

    // 2. Insert Documents with Embeddings + Metadata (including text for hybrid search)
    col.insert(
        "doc-1",
        &[1.0, 0.0, 0.0, 0.0],
        Some(json!({"text": "the quick brown fox", "category": "nature"})),
    )
    .await
    .expect("insert 1");

    col.insert(
        "doc-2",
        &[0.0, 1.0, 0.0, 0.0],
        Some(json!({"text": "jumps over the lazy dog", "category": "action"})),
    )
    .await
    .expect("insert 2");

    col.insert(
        "doc-3",
        &[0.9, 0.1, 0.0, 0.0],
        Some(json!({"text": "the fast brown wolf", "category": "nature"})),
    )
    .await
    .expect("insert 3");

    // 3. Hybrid Search (Vector + Text)
    // Querying for "fox" with a vector close to doc-1
    let query_vector = vec![1.0, 0.1, 0.0, 0.0];
    let results = col
        .hybrid_search("fox", &query_vector, 5)
        .await
        .expect("hybrid search");

    // 4. Verify results
    assert!(!results.is_empty(), "Results should not be empty");
    assert_eq!(
        results[0].id, "doc-1",
        "doc-1 should be the top result for 'fox' and its vector"
    );
    assert!(results[0].score > 0.0);
    assert_eq!(results[0].metadata.as_ref().unwrap()["category"], "nature");

    // 5. Update a document
    col.update(
        "doc-2",
        &[0.0, 0.0, 1.0, 0.0],
        Some(json!({"text": "a sleeping lazy cat", "category": "nature"})),
    )
    .await
    .expect("update");

    // Verify update (search for "cat")
    let results_cat = col
        .hybrid_search("cat", &[0.0, 0.0, 1.0, 0.0], 1)
        .await
        .expect("search cat");
    assert_eq!(results_cat[0].id, "doc-2");
    assert_eq!(
        results_cat[0].metadata.as_ref().unwrap()["text"],
        "a sleeping lazy cat"
    );

    // 6. Relate documents
    col.relate("doc-1", "doc-3", "similar_to")
        .await
        .expect("relate");

    // Verify relation (scan prefix)
    let relations = col
        .scan_prefix("__rel:doc-1:similar_to:")
        .await
        .expect("scan relations");
    assert_eq!(relations.len(), 1);
    assert_eq!(relations[0].1["to"], "doc-3");

    // 7. Delete document
    col.delete("doc-1").await.expect("delete");

    // Verify gone
    let doc1 = col.get("doc-1").await.expect("get doc-1");
    assert!(doc1.is_none());

    let results_final = col
        .search(&[1.0, 0.0, 0.0, 0.0], 10)
        .await
        .expect("search after delete");
    assert!(results_final.iter().all(|r| r.id != "doc-1"));
}
