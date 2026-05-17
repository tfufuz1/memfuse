use memfuse_db::{MemFuse, MemFuseConfig, FilterExpr, DistanceMetric};
use serde_json::json;
use tempfile::TempDir;

async fn setup_db(dim: usize) -> (MemFuse, TempDir) {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: dim,
        max_elements: 1000,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(tmp.path(), config).await.expect("open db");
    (db, tmp)
}

#[tokio::test(flavor = "multi_thread")]
async fn test_filtered_search_post_filter() {
    let (db, _tmp) = setup_db(4).await;

    // Insert documents with different metadata
    for i in 0..10 {
        let topic = if i % 2 == 0 { "rust" } else { "python" };
        db.insert(
            &format!("doc-{}", i),
            &[1.0, 0.0, 0.0, 0.0],
            Some(json!({"topic": topic, "id": i})),
        ).await.expect("insert");
    }

    // Filter for topic "rust"
    let filter = FilterExpr::Eq("topic".to_string(), json!("rust"));
    let results = db.search_with_filter(&[1.0, 0.0, 0.0, 0.0], 10, filter).await.expect("search");

    assert_eq!(results.len(), 5);
    for r in results {
        assert_eq!(r.metadata.expect("meta")["topic"], "rust");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_filtered_search_pre_filter() {
    let (db, _tmp) = setup_db(4).await;

    // Insert many documents to ensure we hit PreFilter logic if needed
    // Actually, Eq has 0.01 selectivity which should trigger PreFilter directly in my implementation
    db.insert("doc-rust", &[1.0, 0.0, 0.0, 0.0], Some(json!({"topic": "rust"}))).await.expect("insert");
    db.insert("doc-python", &[0.0, 1.0, 0.0, 0.0], Some(json!({"topic": "python"}))).await.expect("insert");

    let filter = FilterExpr::Eq("topic".to_string(), json!("rust"));
    let results = db.search_with_filter(&[1.0, 0.0, 0.0, 0.0], 10, filter).await.expect("search");

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "doc-rust");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_complex_filter_and() {
    let (db, _tmp) = setup_db(4).await;

    db.insert("d1", &[1.0, 0.0, 0.0, 0.0], Some(json!({"t": "a", "v": 10}))).await.expect("i");
    db.insert("d2", &[1.0, 0.0, 0.0, 0.0], Some(json!({"t": "a", "v": 20}))).await.expect("i");
    db.insert("d3", &[1.0, 0.0, 0.0, 0.0], Some(json!({"t": "b", "v": 20}))).await.expect("i");

    let filter = FilterExpr::And(vec![
        FilterExpr::Eq("t".to_string(), json!("a")),
        FilterExpr::Gt("v".to_string(), json!(15)),
    ]);

    let results = db.search_with_filter(&[1.0, 0.0, 0.0, 0.0], 10, filter).await.expect("s");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "d2");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_hybrid_search_with_filter() {
    let (db, _tmp) = setup_db(4).await;

    db.insert("rust-1", &[1.0, 0.0, 0.0, 0.0], Some(json!({"topic": "rust", "text": "high performance rust code"}))).await.expect("i");
    db.insert("rust-2", &[1.0, 0.0, 0.0, 0.0], Some(json!({"topic": "rust", "text": "rust safe concurrency"}))).await.expect("i");
    db.insert("python-1", &[1.0, 0.0, 0.0, 0.0], Some(json!({"topic": "python", "text": "python for data science"}))).await.expect("i");

    let filter = FilterExpr::Eq("topic".to_string(), json!("rust"));

    let results = db.hybrid_search_with_filter(
        "performance",
        &[1.0, 0.0, 0.0, 0.0],
        10,
        Some(filter)
    ).await.expect("search");

    assert_eq!(results.len(), 2);
    for r in results {
        assert_eq!(r.metadata.expect("meta")["topic"], "rust");
    }
}
