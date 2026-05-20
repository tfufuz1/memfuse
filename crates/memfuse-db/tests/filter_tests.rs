use memfuse_db::{json, DistanceMetric, FilterExpr, MemFuse, MemFuseConfig};
use tempfile::TempDir;

#[tokio::test]
async fn test_post_filter_returns_only_matching() {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        max_elements: 1000,
        distance_metric: DistanceMetric::Cosine,
        encryption_passphrase: None,
    };
    let db = MemFuse::open_with_config(tmp.path(), config)
        .await
        .expect("open");
    let col = db.collection("filter-test").await.expect("col");

    for i in 0..100 {
        let topic = if i % 2 == 0 { "rust" } else { "python" };
        col.insert(
            &format!("doc-{}", i),
            &[1.0, 0.0, 0.0, 0.0],
            Some(json!({"topic": topic, "index": i})),
        )
        .await
        .expect("insert");
    }

    let filter = FilterExpr::Eq("topic".to_string(), json!("rust"));
    // Since we have 100 docs, it should use the brute force path (< 1000)
    let results = col
        .search_with_filter(&[1.0, 0.0, 0.0, 0.0], 100, filter)
        .await
        .expect("search");

    assert_eq!(results.len(), 50);
    for r in results {
        assert_eq!(r.metadata.unwrap()["topic"], "rust");
    }
}

#[tokio::test]
async fn test_complex_filter_logic() {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        max_elements: 100,
        distance_metric: DistanceMetric::Cosine,
        encryption_passphrase: None,
    };
    let db = MemFuse::open_with_config(tmp.path(), config)
        .await
        .expect("open");
    let col = db.collection("complex-filter").await.expect("col");

    col.insert(
        "d1",
        &[1.0, 0.0, 0.0, 0.0],
        Some(json!({"val": 10, "tag": "a"})),
    )
    .await
    .unwrap();
    col.insert(
        "d2",
        &[1.0, 0.0, 0.0, 0.0],
        Some(json!({"val": 20, "tag": "a"})),
    )
    .await
    .unwrap();
    col.insert(
        "d3",
        &[1.0, 0.0, 0.0, 0.0],
        Some(json!({"val": 30, "tag": "b"})),
    )
    .await
    .unwrap();

    // (val > 15) AND (tag == "a") -> only d2
    let filter = FilterExpr::And(vec![
        FilterExpr::Gt("val".to_string(), json!(15)),
        FilterExpr::Eq("tag".to_string(), json!("a")),
    ]);

    let results = col
        .search_with_filter(&[1.0, 0.0, 0.0, 0.0], 10, filter)
        .await
        .expect("search");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "d2");
}
