use memfuse_db::{MemFuse, MemFuseConfig};
use tempfile::TempDir;
use serde_json::json;

#[tokio::test]
async fn test_hybrid_search_fusion() {
    let tmp = TempDir::new().unwrap();
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(tmp.path(), config).await.unwrap();
    let col = db.collection("search-test").await.unwrap();

    // 1. Insert documents with varied text and vectors
    // doc-1: matches "rust" keyword, vector is away from query
    col.insert("doc-1", &[0.0, 0.0, 0.0, 1.0], Some(json!({"text": "I love rust programming"}))).await.unwrap();

    // doc-2: matches "rust" keyword, vector is close to query
    col.insert("doc-2", &[1.0, 0.0, 0.0, 0.0], Some(json!({"text": "Rust is fast"}))).await.unwrap();

    // doc-3: no "rust" keyword, vector is close to query
    col.insert("doc-3", &[0.9, 0.1, 0.0, 0.0], Some(json!({"text": "Python is slow"}))).await.unwrap();

    // Query: text="rust", vector near [1,0,0,0]
    let query_vec = [1.0, 0.0, 0.0, 0.0];
    let results = col.hybrid_search("rust", &query_vec, 3).await.expect("Hybrid search failed");

    // doc-2 should be #1 because it matches both keyword and vector
    assert_eq!(results[0].id, "doc-2");

    // doc-1 and doc-3 should follow. doc-1 has keyword match, doc-3 has vector match.
    // Reciprocal Rank Fusion (RRF) should handle their ranking.
    let ids: Vec<String> = results.iter().map(|r| r.id.clone()).collect();
    assert!(ids.contains(&"doc-1".to_string()));
    assert!(ids.contains(&"doc-3".to_string()));
}

#[tokio::test]
async fn test_hybrid_search_vector_only() {
    let tmp = TempDir::new().unwrap();
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(tmp.path(), config).await.unwrap();
    let col = db.collection("vector-only").await.unwrap();

    col.insert("v1", &[1.0, 0.0, 0.0, 0.0], None).await.unwrap();
    col.insert("v2", &[0.0, 1.0, 0.0, 0.0], None).await.unwrap();

    // Empty text should fall back to vector search
    let results = col.hybrid_search("", &[1.0, 0.0, 0.0, 0.0], 1).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "v1");
}

#[tokio::test]
async fn test_hybrid_search_text_only() {
    let tmp = TempDir::new().unwrap();
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(tmp.path(), config).await.unwrap();
    let col = db.collection("text-only").await.unwrap();

    col.insert("t1", &[0.0, 0.0, 0.0, 0.0], Some(json!({"text": "apple banana"}))).await.unwrap();
    col.insert("t2", &[0.0, 0.0, 0.0, 0.0], Some(json!({"text": "orange grape"}))).await.unwrap();

    // Zero vector should fall back to text search
    let zero_vec = [0.0; 4];
    let results = col.hybrid_search("apple", &zero_vec, 1).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "t1");
}
