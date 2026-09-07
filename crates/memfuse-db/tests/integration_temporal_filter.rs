use memfuse_core::TxId;
use memfuse_db::{Collection, DistanceMetric, Language};
use memfuse_graph::CsrGraph;
use memfuse_index::{HnswConfig, HnswIndex};
use memfuse_store::{LsmConfig, LsmStorage};
use serde_json::json;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use tempfile::TempDir;

async fn create_test_collection(name: &str) -> (Collection<LsmStorage, HnswIndex>, TempDir) {
    let dir = TempDir::new().unwrap();
    let lsm_config = LsmConfig {
        path: dir.path().to_path_buf(),
        ..Default::default()
    };
    let storage = Arc::new(LsmStorage::new(lsm_config).await.unwrap());
    let hnsw_config = HnswConfig {
        dimension: 4,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };
    let index = Arc::new(HnswIndex::try_new(hnsw_config).unwrap());
    let graph = Arc::new(CsrGraph::new());
    let next_tx = Arc::new(AtomicU64::new(1));
    let col = Collection::new(
        name.to_string(),
        storage,
        index,
        graph,
        next_tx,
        4,
        Language::English,
    );
    (col, dir)
}

#[tokio::test]
async fn test_integration_temporal_filter_valid_until_in_past() {
    let (col, _dir) = create_test_collection("test_past").await;

    col.insert(
        "doc-expired",
        &[1.0, 0.0, 0.0, 0.0],
        Some(json!({ "valid_until": 100 })),
    )
    .await
    .unwrap();

    col.insert(
        "doc-valid",
        &[0.9, 0.1, 0.0, 0.0],
        Some(json!({ "valid_until": 300 })),
    )
    .await
    .unwrap();

    let results = col
        .query()
        .embedding([1.0, 0.0, 0.0, 0.0])
        .query_timestamp(200)
        .k(10)
        .execute()
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "doc-valid");
}

#[tokio::test]
async fn test_integration_temporal_filter_fail_open() {
    let (col, _dir) = create_test_collection("test_fail_open").await;

    col.insert("doc-legacy-no-meta", &[1.0, 0.0, 0.0, 0.0], None)
        .await
        .unwrap();

    col.insert(
        "doc-legacy-custom-meta",
        &[0.9, 0.1, 0.0, 0.0],
        Some(json!({ "category": "engineering" })),
    )
    .await
    .unwrap();

    let results = col
        .query()
        .embedding([1.0, 0.0, 0.0, 0.0])
        .query_timestamp(500)
        .k(10)
        .execute()
        .await
        .unwrap();

    assert_eq!(
        results.len(),
        2,
        "Fail-open behaviour preserves legacy documents without temporal window metadata"
    );
}

#[tokio::test]
async fn test_integration_temporal_filter_as_of_superseded_chunk() {
    let (col, _dir) = create_test_collection("test_as_of_superseded").await;

    // Old chunk valid in [10, 50)
    col.insert(
        "chunk-old-v1",
        &[1.0, 0.0, 0.0, 0.0],
        Some(json!({
            "valid_from": 10,
            "valid_until": 50,
            "tx_valid_from": 10,
            "tx_valid_to": 50
        })),
    )
    .await
    .unwrap();

    // New chunk valid in [50, None)
    col.insert(
        "chunk-new-v2",
        &[0.95, 0.05, 0.0, 0.0],
        Some(json!({
            "valid_from": 50,
            "tx_valid_from": 50
        })),
    )
    .await
    .unwrap();

    // Query historical point in time at t=30
    let historical_30 = col
        .query()
        .embedding([1.0, 0.0, 0.0, 0.0])
        .as_of(30)
        .k(10)
        .execute()
        .await
        .unwrap();

    assert_eq!(historical_30.len(), 1);
    assert_eq!(
        historical_30[0].id, "chunk-old-v1",
        "Historical query at t=30 returns old chunk"
    );

    // Query current/future time at t=60
    let current_60 = col
        .query()
        .embedding([1.0, 0.0, 0.0, 0.0])
        .as_of(60)
        .k(10)
        .execute()
        .await
        .unwrap();

    assert_eq!(current_60.len(), 1);
    assert_eq!(
        current_60[0].id, "chunk-new-v2",
        "Query at t=60 returns new chunk"
    );
}

#[tokio::test]
async fn test_integration_bitemporal_and_logic() {
    let (col, _dir) = create_test_collection("test_bitemporal_and").await;

    // Document refers to business time [10, 100), but ingested at system tx=50
    col.insert(
        "doc-late-ingested",
        &[1.0, 0.0, 0.0, 0.0],
        Some(json!({
            "business_valid_from": 10,
            "business_valid_to": 100,
            "tx_valid_from": 50
        })),
    )
    .await
    .unwrap();

    // Query at system tx=30, business t=30
    let results_at_30 = col
        .query()
        .embedding([1.0, 0.0, 0.0, 0.0])
        .current_tx(TxId::new(30))
        .query_timestamp(30)
        .k(10)
        .execute()
        .await
        .unwrap();

    assert!(
        results_at_30.is_empty(),
        "Bi-temporal AND requires candidate to be valid in both business and transaction dimensions"
    );

    // Query at system tx=60, business t=60
    let results_at_60 = col
        .query()
        .embedding([1.0, 0.0, 0.0, 0.0])
        .current_tx(TxId::new(60))
        .query_timestamp(60)
        .k(10)
        .execute()
        .await
        .unwrap();

    assert_eq!(
        results_at_60.len(),
        1,
        "Valid in both dimensions at tx=60, t=60"
    );
    assert_eq!(results_at_60[0].id, "doc-late-ingested");
}
