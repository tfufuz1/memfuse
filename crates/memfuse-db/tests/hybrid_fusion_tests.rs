use memfuse_db::{filter::FilterOp, HybridQuery, MemFuse, MemFuseConfig, MetadataFilter};
use serde_json::json;
use tempfile::TempDir;

async fn setup_db(dim: usize) -> (MemFuse, TempDir) {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: dim,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(tmp.path(), config)
        .await
        .expect("open db");
    (db, tmp)
}

#[tokio::test]
async fn test_hybrid_vector_text_fusion() {
    let (db, _tmp) = setup_db(4).await;

    // doc-1: high vector match, no text match
    db.insert(
        "doc-1",
        &[1.0, 0.0, 0.0, 0.0],
        Some(json!({"text": "something else"})),
    )
    .await
    .unwrap();

    // doc-2: low vector match, high text match
    db.insert(
        "doc-2",
        &[0.0, 1.0, 0.0, 0.0],
        Some(json!({"text": "rust programming language"})),
    )
    .await
    .unwrap();

    let query = HybridQuery {
        vector: Some(vec![1.0, 0.0, 0.0, 0.0]),
        text: Some("rust".to_string()),
        limit: 10,
        ..Default::default()
    };

    let results = db.hybrid_search_v2(query).await.expect("hybrid search");

    assert_eq!(results.len(), 2);
    // Both should be present. The order depends on RRF scores.
    let ids: Vec<String> = results.iter().map(|r| r.id.clone()).collect();
    assert!(ids.contains(&"doc-1".to_string()));
    assert!(ids.contains(&"doc-2".to_string()));
}

#[tokio::test]
async fn test_hybrid_with_metadata_filter() {
    let (db, _tmp) = setup_db(4).await;

    db.insert(
        "doc-1",
        &[1.0, 0.0, 0.0, 0.0],
        Some(json!({"topic": "rust", "text": "rust is great"})),
    )
    .await
    .unwrap();

    db.insert(
        "doc-2",
        &[1.0, 0.0, 0.0, 0.0],
        Some(json!({"topic": "python", "text": "python is great"})),
    )
    .await
    .unwrap();

    let filter = MetadataFilter::Condition {
        field: "topic".to_string(),
        op: FilterOp::Eq,
        value: json!("rust"),
    };

    let query = HybridQuery {
        vector: Some(vec![1.0, 0.0, 0.0, 0.0]),
        text: Some("great".to_string()),
        metadata_filter: Some(filter),
        limit: 10,
        ..Default::default()
    };

    let results = db.hybrid_search_v2(query).await.expect("hybrid search");

    // Only doc-1 should remain after filtering
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "doc-1");
}

#[tokio::test]
async fn test_hybrid_graph_signal_boost() {
    let (db, _tmp) = setup_db(4).await;

    // doc-1 (seed)
    db.insert("doc-1", &[1.0, 0.0, 0.0, 0.0], None)
        .await
        .unwrap();
    // doc-2 (related to doc-1)
    db.insert("doc-2", &[0.0, 1.0, 0.0, 0.0], None)
        .await
        .unwrap();
    // doc-3 (unrelated)
    db.insert("doc-3", &[0.0, 0.0, 1.0, 0.0], None)
        .await
        .unwrap();

    db.relate("doc-1", "doc-2", "references").await.unwrap();

    // Note: Graph is now automatically refreshed in db.relate()

    let query = HybridQuery {
        graph_seed: Some(("doc-1".to_string(), 1)),
        limit: 10,
        ..Default::default()
    };

    let results = db.hybrid_search_v2(query).await.expect("hybrid search");

    let ids: Vec<String> = results.iter().map(|r| r.id.clone()).collect();
    assert!(ids.contains(&"doc-1".to_string()));
    assert!(ids.contains(&"doc-2".to_string()));
    assert!(!ids.contains(&"doc-3".to_string()));

    // doc-1 should have higher score (1.0) than doc-2 (0.7)
    assert_eq!(results[0].id, "doc-1");
    assert_eq!(results[1].id, "doc-2");
    assert!(results[0].score > results[1].score);
}
