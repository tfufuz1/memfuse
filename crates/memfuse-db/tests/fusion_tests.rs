use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig, HybridQuery, FilterExpr, json};
use tempfile::TempDir;

#[tokio::test]
async fn test_unified_4_signal_fusion() {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        max_elements: 100,
        distance_metric: DistanceMetric::Cosine,
        encryption_passphrase: None,
    };
    let db = MemFuse::open_with_config(tmp.path(), config).await.expect("open");
    let col = db.collection("fusion-test").await.expect("col");

    // doc-1: matches vector perfectly, no text match
    col.insert("doc-1", &[1.0, 0.0, 0.0, 0.0], Some(json!({"text": "something else", "type": "A"}))).await.unwrap();

    // doc-2: matches text perfectly ("rust"), vector is far
    col.insert("doc-2", &[0.0, 0.0, 0.1, 0.9], Some(json!({"text": "Learning Rust is great", "type": "B"}))).await.unwrap();

    // doc-3: matches both somewhat
    col.insert("doc-3", &[0.8, 0.2, 0.0, 0.0], Some(json!({"text": "Rust performance", "type": "A"}))).await.unwrap();

    let query = HybridQuery {
        vector: Some(vec![1.0, 0.0, 0.0, 0.0]),
        text: Some("Rust".to_string()),
        graph_seed: None,
        metadata_filter: Some(FilterExpr::Eq("type".to_string(), json!("A"))),
        weights: None,
        limit: 5,
    };

    let results = col.unified_search(query).await.expect("unified search");

    // Should only contain docs of type "A" (doc-1 and doc-3)
    // doc-2 is type "B" so it should be filtered out.
    assert_eq!(results.len(), 2);
    let ids: Vec<String> = results.iter().map(|r| r.id.clone()).collect();
    assert!(ids.contains(&"doc-1".to_string()));
    assert!(ids.contains(&"doc-3".to_string()));
    assert!(!ids.contains(&"doc-2".to_string()));
}
