//! Acceptance tests for WP-4.2 Advanced Metadata Filtering.

use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use memfuse_db::filter::MetadataFilter;
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
    let db = MemFuse::open_with_config(tmp.path(), config)
        .await
        .expect("open db");
    (db, tmp)
}

#[tokio::test(flavor = "multi_thread")]
async fn test_post_filter_returns_only_matching() {
    let (db, _tmp) = setup_db(4).await;

    // Insert 10 documents, 5 with topic "rust", 5 with topic "python"
    for i in 0..10 {
        let topic = if i % 2 == 0 { "rust" } else { "python" };
        db.insert(
            &format!("doc-{}", i),
            &[1.0, 0.0, 0.0, 0.0],
            Some(json!({"topic": topic, "id": i})),
        ).await.unwrap();
    }

    // Filter for topic == "rust"
    let filter = MetadataFilter::Eq("topic".to_string(), json!("rust"));

    // Search with k=10, should only return the 5 rust documents
    let results = db.search_with_filter(&[1.0, 0.0, 0.0, 0.0], &filter, 10).await.unwrap();

    assert_eq!(results.len(), 5);
    for r in results {
        assert_eq!(r.metadata.unwrap()["topic"], "rust");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_pre_filter_with_low_selectivity() {
    let (db, _tmp) = setup_db(4).await;

    // Insert 100 documents, only 2 match the filter
    for i in 0..100 {
        let category = if i < 2 { "special" } else { "normal" };
        db.insert(
            &format!("doc-{}", i),
            &[1.0, 0.0, 0.0, 0.0],
            Some(json!({"category": category})),
        ).await.unwrap();
    }

    // Filter for category == "special"
    let filter = MetadataFilter::Eq("category".to_string(), json!("special"));

    // Search with k=10.
    // Total docs = 100, k=10. Heuristic: k/total = 0.1 -> should use Pre-filtering.
    // Even if it uses post-filtering with 5x oversample (50 docs), it might find them.
    // But Pre-filtering GUARANTEES it finds them if they exist in the graph.
    let results = db.search_with_filter(&[1.0, 0.0, 0.0, 0.0], &filter, 10).await.unwrap();

    assert_eq!(results.len(), 2);
    let ids: Vec<String> = results.iter().map(|r| r.id.clone()).collect();
    assert!(ids.contains(&"doc-0".to_string()));
    assert!(ids.contains(&"doc-1".to_string()));
}

#[tokio::test(flavor = "multi_thread")]
async fn test_complex_logical_filter() {
    let (db, _tmp) = setup_db(4).await;

    db.insert("d1", &[1.0, 0.0, 0.0, 0.0], Some(json!({"val": 10, "tag": "a"}))).await.unwrap();
    db.insert("d2", &[1.0, 0.0, 0.0, 0.0], Some(json!({"val": 20, "tag": "a"}))).await.unwrap();
    db.insert("d3", &[1.0, 0.0, 0.0, 0.0], Some(json!({"val": 30, "tag": "b"}))).await.unwrap();

    // (tag == "a" AND val > 15) OR (tag == "b")
    let filter = MetadataFilter::Or(vec![
        MetadataFilter::And(vec![
            MetadataFilter::Eq("tag".to_string(), json!("a")),
            MetadataFilter::Gt("val".to_string(), json!(15)),
        ]),
        MetadataFilter::Eq("tag".to_string(), json!("b")),
    ]);

    let results = db.search_with_filter(&[1.0, 0.0, 0.0, 0.0], &filter, 10).await.unwrap();

    // Should return d2 and d3
    assert_eq!(results.len(), 2);
    let ids: Vec<String> = results.iter().map(|r| r.id.clone()).collect();
    assert!(ids.contains(&"d2".to_string()));
    assert!(ids.contains(&"d3".to_string()));
    assert!(!ids.contains(&"d1".to_string()));
}
