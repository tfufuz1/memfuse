use memfuse_db::{MemFuse, MemFuseConfig};
use tempfile::TempDir;

// ANCHOR:INTEGRATION:E2E-001 — Full Stack E2E Test
// AGENT:12 DATE:2026-05-09 STATUS:DONE
#[tokio::test]
async fn test_e2e_full_stack_workflow() {
    let tmp = TempDir::new().unwrap();
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(tmp.path(), config).await.unwrap();

    // 1. Collection Isolation & Insert
    let col1 = db.collection("col1").await.unwrap();
    let col2 = db.collection("col2").await.unwrap();

    col1.insert("doc-c1", &[1.0, 0.0, 0.0, 0.0], Some(serde_json::json!({"text": "rust programming", "tag": "tech"}))).await.unwrap();
    col2.insert("doc-c2", &[0.0, 1.0, 0.0, 0.0], Some(serde_json::json!({"text": "gardening tips", "tag": "hobby"}))).await.unwrap();

    // 2. Retrieval & Isolation Verification
    let d1_c1 = col1.get("doc-c1").await.unwrap().unwrap();
    let d1_c2 = col2.get("doc-c2").await.unwrap().unwrap();
    assert_eq!(d1_c1.metadata.unwrap()["tag"], "tech");
    assert_eq!(d1_c2.metadata.unwrap()["tag"], "hobby");

    assert!(col1.get("doc-c2").await.unwrap().is_none());
    assert!(col2.get("doc-c1").await.unwrap().is_none());

    // 3. Hybrid Search
    // Search in col1
    let results = col1.hybrid_search("rust", &[1.0, 0.0, 0.0, 0.0], 5).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "doc-c1");

    // Search in col2 with text that only exists in col1
    let results_c2 = col2.hybrid_search("rust", &[0.0; 4], 5).await.unwrap();
    assert_eq!(results_c2.len(), 0, "col2 should not find col1's text data");

    // 4. Update
    col1.update("doc-c1", &[1.0, 0.0, 0.0, 0.0], Some(serde_json::json!({"text": "rust language", "tag": "tech"}))).await.unwrap();
    let results_upd = col1.hybrid_search("language", &[1.0, 0.0, 0.0, 0.0], 5).await.unwrap();
    assert_eq!(results_upd.len(), 1);
    assert_eq!(results_upd[0].id, "doc-c1");

    // 5. Delete
    col1.delete("doc-c1").await.unwrap();
    assert!(col1.get("doc-c1").await.unwrap().is_none());
    assert_eq!(col1.len().await, 0);

    // col2 should still have its doc
    assert!(col2.get("doc-c2").await.unwrap().is_some());
    assert_eq!(col2.len().await, 1);
}
