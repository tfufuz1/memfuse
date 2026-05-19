//! Hybrid Search E2E tests for MemFuse.
// ANCHOR:INTEGRATION:HYBRID-001 STATUS:READY AGENT:12 DATE:2026-05-19

use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use tempfile::TempDir;

#[tokio::test]
async fn test_hybrid_search_ranking() {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 3,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(tmp.path(), config)
        .await
        .expect("open db");
    let col = db.collection("hybrid-test").await.expect("collection");

    // Insert documents with specific text and vectors
    // Doc 1: Good text match, mediocre vector match
    col.insert(
        "doc1",
        &[0.5, 0.5, 0.0],
        Some(json!({"text": "The quick brown fox jumps over the lazy dog"})),
    )
    .await
    .unwrap();

    // Doc 2: Mediocre text match, excellent vector match
    col.insert(
        "doc2",
        &[1.0, 0.0, 0.0],
        Some(json!({"text": "A fast animal leaps"})),
    )
    .await
    .unwrap();

    // Doc 3: Irrelevant text, excellent vector match
    col.insert(
        "doc3",
        &[0.99, 0.01, 0.0],
        Some(json!({"text": "Something completely different"})),
    )
    .await
    .unwrap();

    // Query: "fox animal" with vector [1.0, 0.0, 0.0]
    // Doc 1 should have high BM25 for "fox"
    // Doc 2 should have decent BM25 for "animal" and high vector score
    // Doc 3 should have high vector score but zero BM25

    let query_vec = [1.0, 0.0, 0.0];
    let results = col
        .hybrid_search("fox animal", &query_vec, 3)
        .await
        .expect("hybrid search");

    assert_eq!(results.len(), 3);

    // Depending on RRF or whatever fusion is used, doc1 and doc2 should be at the top
    // Let's verify they are present and have scores
    let ids: Vec<String> = results.iter().map(|r| r.id.clone()).collect();
    assert!(ids.contains(&"doc1".to_string()));
    assert!(ids.contains(&"doc2".to_string()));
    assert!(ids.contains(&"doc3".to_string()));

    // Usually doc2 should win because it has both signals.
    // If Doc1 is #1, it means BM25 is very strong.
    // If Doc2 is #1, it's a good hybrid.
    // Doc 3 should be #3 because it lacks text signal.
    assert_eq!(
        results[2].id, "doc3",
        "Doc3 should be ranked last due to lack of text match"
    );
}
