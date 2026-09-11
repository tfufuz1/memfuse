// FILE-CONTEXT: Integration test verifying dimension mismatch rejection, no-panic guarantee, absence of phantom nodes, and system recovery. (TS: 2026-09-11)

use memfuse_core::MemFuseError;
use memfuse_db::{MemFuse, MemFuseConfig};
use serde_json::json;
use tempfile::tempdir;

#[tokio::test]
async fn test_embedding_model_switch_rejects_dimension_mismatch_cleanly() {
    let dir = tempdir().expect("tempdir creation failed");

    // 1. Initialize DB with dimension 1536
    let config = MemFuseConfig {
        dimension: 1536,
        ..Default::default()
    };

    let db = MemFuse::open_with_config(dir.path(), config)
        .await
        .expect("MemFuse initialization failed");

    // Get collection reference
    let collection = db.collection("test_coll").await.expect("collection failed");

    // 2. Perform an initial valid insertion with dimension 1536
    let valid_vec1536_1 = vec![0.1f32; 1536];
    collection
        .insert("doc1", &valid_vec1536_1, Some(json!({"title": "Doc 1"})))
        .await
        .expect("insert valid_vec1536_1 failed");

    // Verify document count
    let count_before = collection.len().await;
    assert_eq!(
        count_before, 1,
        "Collection must contain 1 document after valid insert"
    );

    // 3. Attempt insertion with mismatched dimension (768) - simulating model switch
    let mismatched_vec768 = vec![0.2f32; 768];
    let insert_mismatched_res = collection
        .insert(
            "doc2_mismatched",
            &mismatched_vec768,
            Some(json!({"title": "Doc 2 Mismatched"})),
        )
        .await;

    // (a) Verify InvalidInput error is returned (validating JULES-10)
    assert!(
        insert_mismatched_res.is_err(),
        "Inserting vector with mismatched dimension must return an Error"
    );
    let err = insert_mismatched_res.err().unwrap();
    match err {
        MemFuseError::InvalidInput(msg) => {
            assert!(
                msg.contains("dimension") || msg.contains("mismatch") || msg.contains("1536"),
                "Error message should mention dimension mismatch: {}",
                msg
            );
        }
        other => panic!("Expected MemFuseError::InvalidInput, got {:?}", other),
    }

    // (b) Verify NO panic occurred and (c) Collection / Index contains NO phantom document for the failed insert
    let count_after = collection.len().await;
    assert_eq!(
        count_after, 1,
        "Collection must NOT create a phantom document or index node for the failed insert"
    );

    let doc2_get = collection.get("doc2_mismatched").await.expect("get failed");
    assert!(
        doc2_get.is_none(),
        "Failed mismatched insertion must leave no document record"
    );

    // (d) Verify subsequent correctly-dimensioned (1536) insert functions normally
    let valid_vec1536_2 = vec![0.3f32; 1536];
    collection
        .insert(
            "doc3_valid",
            &valid_vec1536_2,
            Some(json!({"title": "Doc 3 Valid"})),
        )
        .await
        .expect("insert valid_vec1536_2 failed");

    // Verify document count and retrieval
    let count_final = collection.len().await;
    assert_eq!(
        count_final, 2,
        "Collection must contain 2 documents after subsequent valid insert"
    );

    let doc3 = collection.get("doc3_valid").await.expect("get doc3 failed");
    assert!(doc3.is_some(), "doc3_valid must be readable");
}
