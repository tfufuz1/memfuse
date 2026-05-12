use memfuse_db::{json, MemFuse, MemFuseConfig};
use tempfile::TempDir;

#[tokio::test]
async fn test_full_document_lifecycle_e2e() {
    let tmp = TempDir::new().unwrap();
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(tmp.path(), config).await.unwrap();

    // 1. Insert documents
    db.insert(
        "doc1",
        &[1.0, 0.0, 0.0, 0.0],
        Some(json!({"text": "apple", "cat": "fruit"})),
    )
    .await
    .unwrap();
    db.insert(
        "doc2",
        &[0.0, 1.0, 0.0, 0.0],
        Some(json!({"text": "banana", "cat": "fruit"})),
    )
    .await
    .unwrap();
    db.insert(
        "doc3",
        &[0.0, 0.0, 1.0, 0.0],
        Some(json!({"text": "carrot", "cat": "vegetable"})),
    )
    .await
    .unwrap();

    // 2. Hybrid search
    let results = db.search(&[1.0, 0.0, 0.0, 0.0], 2).await.unwrap();
    assert_eq!(results[0].id, "doc1");

    // 3. Update document
    db.update(
        "doc3",
        &[1.0, 0.1, 0.0, 0.0],
        Some(json!({"text": "red carrot", "cat": "vegetable"})),
    )
    .await
    .unwrap();

    // Search again, doc3 should be closer now
    let results = db.search(&[1.0, 0.0, 0.0, 0.0], 2).await.unwrap();
    assert!(results.iter().any(|r| r.id == "doc3"));

    // 4. Delete document
    db.delete("doc1").await.unwrap();
    let doc = db.get("doc1").await.unwrap();
    assert!(doc.is_none());

    // 5. Collection Isolation
    let col_other = db.collection("other").await.unwrap();
    col_other
        .insert("doc1", &[0.5, 0.5, 0.5, 0.5], None)
        .await
        .unwrap();

    let doc_in_default = db.get("doc1").await.unwrap();
    assert!(
        doc_in_default.is_none(),
        "doc1 should still be gone from default"
    );

    let doc_in_other = col_other.get("doc1").await.unwrap();
    assert!(
        doc_in_other.is_some(),
        "doc1 should exist in other collection"
    );
}
