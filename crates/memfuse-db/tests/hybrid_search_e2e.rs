//! End-to-End Hybrid Search tests for MemFuse.
// ANCHOR:INTEGRATION:HYBRID-001 STATUS:READY AGENT:12 DATE:2026-05-21

use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use tempfile::TempDir;

#[tokio::test]
async fn test_hybrid_search_ranking_and_fusion() {
    let tmp = TempDir::new().expect("Failed to create temp dir");
    let config = MemFuseConfig {
        dimension: 2,
        distance_metric: DistanceMetric::Euclidean, // Use Euclidean for easier mental math
        ..Default::default()
    };

    let db = MemFuse::open_with_config(tmp.path(), config)
        .await
        .expect("Failed to open DB");

    let col = db.collection("hybrid").await.expect("collection");

    // doc1: Good text match ("rust"), decent vector match
    col.insert(
        "doc1",
        &[1.0, 0.0],
        Some(json!({"text": "Rust is a systems language"})),
    )
    .await
    .expect("ins");

    // doc2: Excellent vector match, poor text match
    col.insert(
        "doc2",
        &[0.0, 1.0],
        Some(json!({"text": "Python is a scripting language"})),
    )
    .await
    .expect("ins");

    // doc3: Decent text match ("rust"), poor vector match
    col.insert(
        "doc3",
        &[5.0, 5.0],
        Some(json!({"text": "I am learning rust too"})),
    )
    .await
    .expect("ins");

    // doc4: No text match, poor vector match
    col.insert(
        "doc4",
        &[10.0, 10.0],
        Some(json!({"text": "Something else entirely"})),
    )
    .await
    .expect("ins");

    // Hybrid search for "rust" and vector [0.9, 0.1]
    // Vector [0.9, 0.1] is closest to doc1 [1, 0].
    // Text "rust" matches doc1 and doc3.

    let query_vec = &[0.9, 0.1];
    let query_text = "rust";

    let results = col
        .hybrid_search(query_text, query_vec, 4)
        .await
        .expect("hybrid search");

    assert!(results.len() >= 3);

    // RRF should favor documents that rank high in BOTH or VERY high in ONE.
    // doc1: High in text, high in vector.
    // doc2: High in vector, low in text (0).
    // doc3: High in text, low in vector.

    let ids: Vec<String> = results.iter().map(|r| r.id.clone()).collect();

    // Check if expected docs are present
    assert!(ids.contains(&"doc1".to_string()));
    assert!(ids.contains(&"doc2".to_string()));
    assert!(ids.contains(&"doc3".to_string()));

    // doc1 should likely be #1 because it's in both result sets (if k is large enough in search)
    // Actually, hybrid_search calls self.search(vector, k) and self.text_index.search_bm25(text, k).
    // With k=4:
    // Vector search [0.1, 0.9]: [doc2, doc1, doc3, doc4]
    // Text search "rust": [doc1, doc3] (or [doc3, doc1] depending on BM25)
    // RRF will combine these. doc1 and doc3 will get boosts from both. doc2 only from vector.

    assert_eq!(
        results[0].id, "doc1",
        "doc1 should be the top result for hybrid search 'rust' + near-doc1/doc2 vector"
    );
}

#[tokio::test]
async fn test_hybrid_search_edge_cases() {
    let tmp = TempDir::new().expect("temp dir");
    let db = MemFuse::open(tmp.path()).await.expect("open");
    let col = db.collection("edges").await.expect("col");

    col.insert("doc1", &vec![1.0; 1536], Some(json!({"text": "hello"})))
        .await
        .expect("ins");

    // 1. Empty text
    let res1 = col
        .hybrid_search("", &vec![1.0; 1536], 1)
        .await
        .expect("empty text");
    assert_eq!(res1.len(), 1);
    assert_eq!(res1[0].id, "doc1");

    // 2. Zero vector (assuming all zeros is "empty")
    let res2 = col
        .hybrid_search("hello", &vec![0.0; 1536], 1)
        .await
        .expect("zero vector");
    assert_eq!(res2.len(), 1);
    assert_eq!(res2[0].id, "doc1");

    // 3. Both empty
    let res3 = col
        .hybrid_search("", &vec![0.0; 1536], 1)
        .await
        .expect("both empty");
    assert!(res3.is_empty());
}
