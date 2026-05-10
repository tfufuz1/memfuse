use memfuse_db::{json, MemFuse, MemFuseConfig};
use tempfile::TempDir;

#[tokio::test]
async fn test_full_stack_lifecycle_e2e() {
    let tmp = TempDir::new().unwrap();
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };

    // 1. MemFuse::open()
    let db = MemFuse::open_with_config(tmp.path(), config).await.unwrap();

    // 2. Insert Dokumente mit Embeddings + Metadata
    db.insert(
        "doc-1",
        &[1.0, 0.0, 0.0, 0.0],
        Some(json!({"text": "semantic search is cool", "category": "tech"})),
    )
    .await
    .unwrap();

    db.insert(
        "doc-2",
        &[0.0, 1.0, 0.0, 0.0],
        Some(json!({"text": "rust programming language", "category": "coding"})),
    )
    .await
    .unwrap();

    // 3. Hybrid Search (Vector + Text)
    // Query that matches doc-2 by text "rust" and doc-1 by vector similarity
    let results = db
        .collection("default")
        .await
        .unwrap()
        .hybrid_search("rust", &[1.0, 0.0, 0.0, 0.0], 10)
        .await
        .unwrap();

    // 4. Verify Ergebnisse (Score, Metadata, Ordering)
    assert!(!results.is_empty());
    // doc-2 matches text "rust", doc-1 matches vector similarity.
    // Ordering depends on RRF. We check if both are present.
    let ids: Vec<String> = results.iter().map(|r| r.id.clone()).collect();
    assert!(ids.contains(&"doc-1".to_string()));
    assert!(ids.contains(&"doc-2".to_string()));

    // 5. Update + Re-Search
    db.update(
        "doc-1",
        &[0.0, 0.0, 1.0, 0.0],
        Some(json!({"text": "updated text", "category": "updated"})),
    )
    .await
    .unwrap();

    let results_updated = db.search(&[0.0, 0.0, 1.0, 0.0], 1).await.unwrap();
    assert_eq!(results_updated[0].id, "doc-1");
    assert_eq!(
        results_updated[0].metadata.as_ref().unwrap()["category"],
        "updated"
    );

    // 6. Delete + Verify Gone
    db.delete("doc-1").await.unwrap();
    let doc_gone = db.get("doc-1").await.unwrap();
    assert!(doc_gone.is_none());

    let results_after_delete = db.search(&[0.0, 0.0, 1.0, 0.0], 10).await.unwrap();
    assert!(results_after_delete.iter().all(|r| r.id != "doc-1"));

    // 7. Collection Isolation
    let col_a = db.collection("alpha").await.unwrap();
    let col_b = db.collection("beta").await.unwrap();

    col_a
        .insert(
            "shared-id",
            &[1.0, 1.0, 1.0, 1.0],
            Some(json!({"col": "a"})),
        )
        .await
        .unwrap();
    col_b
        .insert(
            "shared-id",
            &[1.0, 1.0, 1.0, 1.0],
            Some(json!({"col": "b"})),
        )
        .await
        .unwrap();

    let doc_a = col_a.get("shared-id").await.unwrap().unwrap();
    let doc_b = col_b.get("shared-id").await.unwrap().unwrap();

    assert_eq!(doc_a.metadata.unwrap()["col"], "a");
    assert_eq!(doc_b.metadata.unwrap()["col"], "b");
}
