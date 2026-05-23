use memfuse_db::{MemFuse, MemFuseConfig};
use tempfile::TempDir;

#[tokio::test]
async fn test_collection_atomic_rollback_on_error() {
    let tmp = TempDir::new().unwrap(); // unwrap allowed
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(tmp.path(), config).await.unwrap(); // unwrap allowed
    let col = db.collection("test_col").await.unwrap(); // unwrap allowed

    // Valid insert
    col.insert("doc1", &[0.1, 0.2, 0.3, 0.4], None)
        .await
        .unwrap(); // unwrap allowed

    // Trigger invalid insert that fails inside index.commit()
    let res = col.insert("doc_invalid", &[f32::NAN; 4], None).await;

    assert!(res.is_err(), "Insert with NaN should fail");

    // Verify that the document was completely rolled back and is not in LSM store
    let retrieved = col.get("doc_invalid").await.unwrap(); // unwrap allowed
    assert!(
        retrieved.is_none(),
        "doc_invalid should not exist in storage due to rollback"
    );

    // Verify it's not in the vector index
    let stats = col.stats().await.unwrap(); // unwrap allowed
    assert_eq!(stats.num_vectors, 1, "Only doc1 should be in the index");

    // Test search for phantom hits
    let results = col.search(&[0.1, 0.2, 0.3, 0.4], 10).await.unwrap(); // unwrap allowed
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "doc1");
}
