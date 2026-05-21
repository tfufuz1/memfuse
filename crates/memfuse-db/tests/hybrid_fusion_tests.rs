use memfuse_db::{
    collection::{FusionWeights, HybridQuery},
    filter::{FilterOp, MetadataFilter},
    MemFuse, MemFuseConfig,
};
use serde_json::json;
use tempfile::TempDir;

#[tokio::test]
async fn test_4_signal_fusion_smoke() {
    let tmp = TempDir::new().unwrap(); // unwrap
    let config = MemFuseConfig {
        dimension: 3,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(tmp.path(), config).await.unwrap(); // unwrap

    // 1. Setup data
    db.insert(
        "doc-1",
        &[1.0, 0.0, 0.0],
        Some(json!({"text": "rust programming", "category": "tech"})),
    )
    .await
    .unwrap(); // unwrap
    db.insert(
        "doc-2",
        &[0.0, 1.0, 0.0],
        Some(json!({"text": "python scripting", "category": "tech"})),
    )
    .await
    .unwrap(); // unwrap
    db.insert(
        "doc-3",
        &[0.0, 0.0, 1.0],
        Some(json!({"text": "gardening tips", "category": "hobby"})),
    )
    .await
    .unwrap(); // unwrap

    db.relate("doc-1", "doc-2", "related").await.unwrap(); // unwrap

    // 2. Query with all 4 signals
    let query = HybridQuery {
        vector: Some(vec![1.0, 0.0, 0.0]),
        text: Some("python".to_string()),
        graph_seed: Some(("doc-1".to_string(), 1)),
        metadata_filter: Some(MetadataFilter::Condition {
            field: "category".to_string(),
            op: FilterOp::Eq,
            value: json!("tech"),
        }),
        weights: FusionWeights::default(),
        limit: 5,
    };

    let results = db.hybrid_search_unified(query).await.unwrap(); // unwrap

    assert!(!results.is_empty());

    // doc-1 matches vector and metadata
    // doc-2 matches text, graph and metadata
    // doc-3 matches metadata only

    let ids: Vec<String> = results.iter().map(|r| r.id.clone()).collect();
    assert!(ids.contains(&"doc-1".to_string()));
    assert!(ids.contains(&"doc-2".to_string()));
    assert!(ids.contains(&"doc-3".to_string()));
}

#[tokio::test]
async fn test_fusion_weights_influence() {
    let tmp = TempDir::new().unwrap(); // unwrap
    let config = MemFuseConfig {
        dimension: 3,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(tmp.path(), config).await.unwrap(); // unwrap

    db.insert("doc-v", &[1.0, 0.0, 0.0], None).await.unwrap(); // unwrap
    db.insert("doc-t", &[0.0, 0.0, 0.0], Some(json!({"text": "keyword"})))
        .await
        .unwrap(); // unwrap

    // Vector heavy
    let query_v = HybridQuery {
        vector: Some(vec![1.0, 0.0, 0.0]),
        text: Some("keyword".to_string()),
        graph_seed: None,
        metadata_filter: None,
        weights: FusionWeights {
            vector: 1.0,
            text: 0.1,
            graph: 0.0,
            metadata: 0.0,
        },
        limit: 1,
    };
    let res_v = db.hybrid_search_unified(query_v).await.unwrap(); // unwrap
    assert_eq!(res_v[0].id, "doc-v");

    // Text heavy
    let query_t = HybridQuery {
        vector: Some(vec![1.0, 0.0, 0.0]),
        text: Some("keyword".to_string()),
        graph_seed: None,
        metadata_filter: None,
        weights: FusionWeights {
            vector: 0.1,
            text: 1.0,
            graph: 0.0,
            metadata: 0.0,
        },
        limit: 1,
    };
    let res_t = db.hybrid_search_unified(query_t).await.unwrap(); // unwrap
    assert_eq!(res_t[0].id, "doc-t");
}
