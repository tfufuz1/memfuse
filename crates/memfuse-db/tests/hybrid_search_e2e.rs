//! Hybrid Search E2E test verifying ranking quality and RRF fusion.
// ANCHOR:INTEGRATION:E2E-004 STATUS:READY AGENT:12 DATE:2026-05-18

use memfuse_db::{MemFuse, MemFuseConfig};
use serde_json::json;
use tempfile::TempDir;

#[tokio::test]
async fn test_hybrid_search_ranking_and_fusion() {
    let tmp = TempDir::new().expect("failed to create temp dir");
    let config = MemFuseConfig {
        dimension: 2,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(tmp.path(), config)
        .await
        .expect("failed to open db");
    let col = db.collection("hybrid-test").await.expect("collection failed");

    // doc1: Vector signal [0.0, 1.0] (opposite of query [1.0, 0.0])
    col.insert("doc1", &[0.0, 1.0], Some(json!({"text": "Irrelevant content here"})))
        .await
        .unwrap();

    // doc2: Good vector match for [1.0, 0.0], but NO text match
    col.insert("doc2", &[1.0, 0.0], Some(json!({"text": "Something else entirely"})))
        .await
        .unwrap();

    // doc3: Good text match for "rust" AND good vector match for [1.0, 0.0]
    col.insert("doc3", &[0.9, 0.1], Some(json!({"text": "The Rust language is for systems programming"})))
        .await
        .unwrap();

    let query_text = "rust";
    let query_vec = [1.0, 0.0];

    // Hybrid search should rank doc3 first as it matches both signals
    let results = col.hybrid_search(query_text, &query_vec, 3).await.expect("hybrid search failed");

    assert_eq!(results.len(), 3);
    assert_eq!(results[0].id, "doc3", "doc3 should be ranked first due to hybrid match. Results: {:?}", results);

    // doc1 and doc2 should follow (order between them depends on RRF and specific scores,
    // but both should be present)
    let ids: Vec<String> = results.iter().map(|r| r.id.clone()).collect();
    assert!(ids.contains(&"doc1".to_string()));
    assert!(ids.contains(&"doc2".to_string()));
}
