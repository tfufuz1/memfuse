//! Comprehensive E2E tests for MemFuse DB.
//! Tests the full stack from Orchestrator down to Storage.

use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use tempfile::TempDir;

#[tokio::test]
async fn test_comprehensive_lifecycle() {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        max_elements: 1000,
        distance_metric: DistanceMetric::Cosine,
    };

    // 1. Open
    let db = MemFuse::open_with_config(tmp.path(), config)
        .await
        .expect("open db");

    // 2. Insert with metadata containing text
    db.insert(
        "doc-1",
        &[1.0, 0.0, 0.0, 0.0],
        Some(json!({"text": "The quick brown fox", "category": "nature"})),
    )
    .await
    .expect("insert 1");

    db.insert(
        "doc-2",
        &[0.0, 1.0, 0.0, 0.0],
        Some(json!({"content": "Jumped over the lazy dog", "category": "nature"})),
    )
    .await
    .expect("insert 2");

    // 3. Hybrid Search - Vector match
    let results = db.search(&[1.0, 0.0, 0.0, 0.0], 1).await.expect("search");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "doc-1");
    assert_eq!(results[0].metadata.as_ref().unwrap()["category"], "nature");

    // 4. Hybrid Search - Text match (BM25)
    let results = db.hybrid_search("lazy dog", &[0.0, 0.0, 0.0, 0.0], 1)
        .await
        .expect("hybrid search text");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "doc-2");

    // 5. Update
    db.update(
        "doc-1",
        &[0.5, 0.5, 0.0, 0.0],
        Some(json!({"text": "The slow blue fox", "category": "nature"})),
    )
    .await
    .expect("update 1");

    // Re-verify update (vector)
    let results = db.search(&[0.5, 0.5, 0.0, 0.0], 1).await.expect("search updated");
    assert_eq!(results[0].id, "doc-1");
    // Re-verify update (text)
    let results = db.hybrid_search("blue", &[0.0, 0.0, 0.0, 0.0], 1)
        .await
        .expect("hybrid search blue");
    assert_eq!(results[0].id, "doc-1");
    let results = db.hybrid_search("quick", &[0.0, 0.0, 0.0, 0.0], 1)
        .await
        .expect("hybrid search quick");
    assert!(results.is_empty() || results[0].id != "doc-1");

    // 6. Multi-Collection Isolation
    let col_a = db.collection("alpha").await.expect("col alpha");
    let col_b = db.collection("beta").await.expect("col beta");

    col_a.insert("secret", &[1.0, 1.0, 1.0, 1.0], Some(json!({"msg": "alpha"})))
        .await.expect("ins alpha");
    col_b.insert("secret", &[1.0, 1.0, 1.0, 1.0], Some(json!({"msg": "beta"})))
        .await.expect("ins beta");

    let doc_a = col_a.get("secret").await.expect("get a").unwrap();
    let doc_b = col_b.get("secret").await.expect("get b").unwrap();
    assert_eq!(doc_a.metadata.unwrap()["msg"], "alpha");
    assert_eq!(doc_b.metadata.unwrap()["msg"], "beta");

    // 7. Persistence
    drop(col_a);
    drop(col_b);
    drop(db);

    let db_reopened = MemFuse::open_with_config(tmp.path(), MemFuseConfig {
        dimension: 4,
        ..Default::default()
    }).await.expect("reopen");

    let col_a = db_reopened.collection("alpha").await.expect("get alpha");
    let doc_a = col_a.get("secret").await.expect("get a reopened").unwrap();
    assert_eq!(doc_a.metadata.unwrap()["msg"], "alpha");

    // 8. Delete
    col_a.delete("secret").await.expect("delete secret");
    assert!(col_a.get("secret").await.expect("get gone").is_none());
}
