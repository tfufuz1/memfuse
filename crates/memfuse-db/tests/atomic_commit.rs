#![allow(deprecated)]

use memfuse_db::{MemFuse, MemFuseConfig};
use tempfile::TempDir;

#[tokio::test]
async fn test_collection_atomic_rollback_on_error() {
    let tmp = TempDir::new().unwrap();
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(tmp.path(), config).await.unwrap();
    let col = db.collection("test_col").await.unwrap();

    // Valid insert
    col.insert("doc1", &[0.1, 0.2, 0.3, 0.4], None)
        .await
        .unwrap();

    // Trigger invalid insert that fails inside index.commit()
    let res = col.insert("doc_invalid", &[f32::NAN; 4], None).await;

    assert!(res.is_err(), "Insert with NaN should fail");

    // Verify that the document was completely rolled back and is not in LSM store
    let retrieved = col.get("doc_invalid").await.unwrap();
    assert!(
        retrieved.is_none(),
        "doc_invalid should not exist in storage due to rollback"
    );

    // Verify it's not in the vector index
    let stats = col.stats().await.unwrap();
    assert_eq!(stats.num_vectors, 1, "Only doc1 should be in the index");

    // Test search for phantom hits
    let results = col.search(&[0.1, 0.2, 0.3, 0.4], 10).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "doc1");
}

#[tokio::test]
async fn test_4_index_atomic_rollback_on_vector_failure() {
    use memfuse_core::EntityId;
    use serde_json::json;

    let tmp = TempDir::new().unwrap();
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(tmp.path(), config).await.unwrap();
    let col = db.collection("four_index_col").await.unwrap();

    // 1. Valid insertion with text and graph
    col.insert(
        "doc_valid",
        &[0.1, 0.2, 0.3, 0.4],
        Some(json!({
            "text": "Valid document text for search"
        })),
    )
    .await
    .unwrap();

    // 2. Failed insertion (NaN embedding triggers vector index commit failure)
    let res = col
        .insert(
            "doc_failed",
            &[f32::NAN; 4],
            Some(json!({
                "text": "Failed document text for search"
            })),
        )
        .await;

    assert!(res.is_err(), "Insert with NaN vector must fail");

    // 3. Verify LSM Storage isolation
    let doc_in_lsm = col.get("doc_failed").await.unwrap();
    assert!(
        doc_in_lsm.is_none(),
        "doc_failed must not be present in LSM storage after rollback"
    );

    // 4. Verify BM25 Text Index isolation (no phantom text hits)
    let hybrid_res = col
        .hybrid_search("Failed", &[0.1, 0.2, 0.3, 0.4], 10, None)
        .await
        .unwrap();
    assert!(
        hybrid_res.iter().all(|r| r.id != "doc_failed"),
        "doc_failed text must not be present in text index"
    );

    // 5. Verify Graph Index isolation
    let eid_failed = EntityId::from_key("doc_failed").unwrap();
    let neighbors = col.graph_index().neighbors(eid_failed).await.unwrap();
    assert!(
        neighbors.is_empty(),
        "doc_failed entity must not be present in graph index"
    );
}
