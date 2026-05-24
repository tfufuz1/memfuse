// ANCHOR:INTEGRATION:E2E-002 STATUS:READY AGENT:12
use memfuse_db::{MemFuse, MemFuseConfig, DistanceMetric};
use serde_json::json;
use tempfile::TempDir;

#[tokio::test]
async fn test_full_stack_lifecycle_e2e() {
    // 1. MemFuse::open()
    let tmp = TempDir::new().expect("failed to create temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        max_elements: 100,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(tmp.path(), config).await.expect("open db failed");
    let col = db.collection("e2e-collection").await.expect("collection failed");

    // 2. Insert Dokumente mit Embeddings + Metadata
    col.insert("doc1", &[1.0, 0.0, 0.0, 0.0], Some(json!({"text": "The quick brown fox", "type": "animal"})))
        .await.expect("insert doc1 failed");
    col.insert("doc2", &[0.0, 1.0, 0.0, 0.0], Some(json!({"text": "Jumped over the lazy dog", "type": "animal"})))
        .await.expect("insert doc2 failed");

    // 3. Hybrid Search (Vector + Text)
    let results = col.hybrid_search("quick", &[1.0, 0.1, 0.0, 0.0], 5).await.expect("search failed");

    // 4. Verify Ergebnisse (Score, Metadata, Ordering)
    assert!(!results.is_empty());
    assert_eq!(results[0].id, "doc1");
    assert_eq!(results[0].metadata.as_ref().unwrap()["type"], "animal");

    // 5. Update + Re-Search
    col.update("doc1", &[1.0, 0.0, 0.0, 0.0], Some(json!({"text": "The very quick brown fox", "type": "mammal"})))
        .await.expect("update failed");
    let results2 = col.hybrid_search("very", &[1.0, 0.0, 0.0, 0.0], 1).await.expect("search failed");
    assert_eq!(results2[0].metadata.as_ref().unwrap()["type"], "mammal");

    // 6. Delete + Verify Gone
    col.delete("doc1").await.expect("delete failed");
    let doc = col.get("doc1").await.expect("get failed");
    assert!(doc.is_none());

    // 7. Collection Isolation
    let col2 = db.collection("other-collection").await.expect("col2 failed");
    col2.insert("doc2", &[0.0, 1.0, 0.0, 0.0], Some(json!({"isolation": "ok"})))
        .await.expect("insert failed");

    let doc_in_col1 = col.get("doc2").await.expect("get failed").expect("missing in col1");
    let doc_in_col2 = col2.get("doc2").await.expect("get failed").expect("missing in col2");

    assert_eq!(doc_in_col1.metadata.unwrap()["text"], "Jumped over the lazy dog");
    assert_eq!(doc_in_col2.metadata.unwrap()["isolation"], "ok");
}
