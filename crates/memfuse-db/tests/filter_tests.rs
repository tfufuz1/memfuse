use memfuse_db::{filter::FilterOp, MemFuse, MemFuseConfig, MetadataFilter};
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
async fn test_post_filter_returns_only_matching() {
    let (db, _tmp) = setup_db(4).await;

    for i in 0..100 {
        let topic = if i % 2 == 0 { "rust" } else { "python" };
        db.insert(
            &format!("doc-{}", i),
            &[1.0, 0.0, 0.0, 0.0],
            Some(json!({"topic": topic, "id": i})),
        )
        .await
        .expect("insert");
    }

    let filter = MetadataFilter::Condition {
        field: "topic".to_string(),
        op: FilterOp::Eq,
        value: json!("rust"),
    };

    let results = db
        .search_with_filter(&[1.0, 0.0, 0.0, 0.0], 10, Some(filter))
        .await
        .expect("search");

    assert!(!results.is_empty());
    for res in results {
        assert_eq!(res.metadata.expect("metadata")["topic"], "rust");
    }
}

#[tokio::test]
async fn test_pre_filter_with_low_selectivity() {
    let (db, _tmp) = setup_db(4).await;

    // 100 docs, only 2 match the filter
    for i in 0..100 {
        let topic = if i < 2 { "special" } else { "common" };
        db.insert(
            &format!("doc-{}", i),
            &[1.0, 0.0, 0.0, 0.0],
            Some(json!({"topic": topic})),
        )
        .await
        .expect("insert");
    }

    let filter = MetadataFilter::Condition {
        field: "topic".to_string(),
        op: FilterOp::Eq,
        value: json!("special"),
    };

    // This should trigger the "pre-filter" (metadata scan) because 100 < 1000
    let results = db
        .search_with_filter(&[1.0, 0.0, 0.0, 0.0], 10, Some(filter))
        .await
        .expect("search");

    assert_eq!(results.len(), 2);
    assert!(results
        .iter()
        .all(|r| r.metadata.as_ref().expect("unwrap")["topic"] == "special"));
}

#[tokio::test]
async fn test_complex_logical_filter() {
    let (db, _tmp) = setup_db(4).await;

    db.insert(
        "d1",
        &[1.0, 0.0, 0.0, 0.0],
        Some(json!({"tags": ["a", "b"], "val": 10})),
    )
    .await
    .expect("unwrap");
    db.insert(
        "d2",
        &[1.0, 0.0, 0.0, 0.0],
        Some(json!({"tags": ["a"], "val": 20})),
    )
    .await
    .expect("unwrap");
    db.insert(
        "d3",
        &[1.0, 0.0, 0.0, 0.0],
        Some(json!({"tags": ["b"], "val": 30})),
    )
    .await
    .expect("unwrap");

    // (val > 15) AND (NOT tags contains "b") -> should only be d2
    let filter = MetadataFilter::And(vec![
        MetadataFilter::Condition {
            field: "val".to_string(),
            op: FilterOp::Gt,
            value: json!(15),
        },
        MetadataFilter::Not(Box::new(MetadataFilter::Condition {
            field: "tags".to_string(),
            op: FilterOp::In,
            value: json!("b"),
        })),
    ]);

    let results = db
        .search_with_filter(&[1.0, 0.0, 0.0, 0.0], 10, Some(filter))
        .await
        .expect("unwrap");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "d2");
}
