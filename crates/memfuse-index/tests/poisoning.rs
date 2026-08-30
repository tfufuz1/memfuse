use memfuse_core::traits::VectorIndex;
use memfuse_core::types::{DocId, TxId};
use memfuse_index::{HnswConfig, HnswIndex};

#[tokio::test]
async fn test_nan_poisoning_prevention() {
    let config = HnswConfig {
        dimension: 4,
        ..Default::default()
    };
    let index = HnswIndex::try_new(config).unwrap();

    let tx = TxId(1);
    let id = DocId(1);
    let embedding = vec![1.0, f32::NAN, 3.0, 4.0];

    let result = index.insert(tx, id, &embedding).await;
    assert!(result.is_err(), "Insertion of NaN should fail");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("NaN or Infinity detected"),
        "Error message should mention NaN/Infinity, got: {}",
        err
    );
}

#[tokio::test]
async fn test_search_rejects_nan_query_vector() {
    let config = HnswConfig {
        dimension: 4,
        ..Default::default()
    };
    let index = HnswIndex::try_new(config).unwrap();

    let result = index.search(&[1.0, f32::NAN, 3.0, 4.0], 5).await;
    assert!(result.is_err(), "Search with NaN query vector should fail");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("Query vector contains NaN or infinite values"),
        "Unexpected error message: {}",
        err
    );
}

#[tokio::test]
async fn test_search_rejects_infinite_query_vector() {
    let config = HnswConfig {
        dimension: 4,
        ..Default::default()
    };
    let index = HnswIndex::try_new(config).unwrap();

    let result = index.search(&[1.0, f32::INFINITY, 3.0, 4.0], 5).await;
    assert!(
        result.is_err(),
        "Search with Infinite query vector should fail"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("Query vector contains NaN or infinite values"),
        "Unexpected error message: {}",
        err
    );

    let result_neg = index.search(&[f32::NEG_INFINITY, 2.0, 3.0, 4.0], 5).await;
    assert!(
        result_neg.is_err(),
        "Search with -Infinity query vector should fail"
    );
    let err_neg = result_neg.unwrap_err().to_string();
    assert!(
        err_neg.contains("Query vector contains NaN or infinite values"),
        "Unexpected error message: {}",
        err_neg
    );
}

#[tokio::test]
async fn test_inf_poisoning_prevention() {
    let config = HnswConfig {
        dimension: 4,
        ..Default::default()
    };
    let index = HnswIndex::try_new(config).unwrap();

    let tx = TxId(1);
    let id = DocId(1);
    let embedding = vec![1.0, 2.0, f32::INFINITY, 4.0];

    let result = index.insert(tx, id, &embedding).await;
    assert!(result.is_err(), "Insertion of Infinity should fail");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("NaN or Infinity detected"),
        "Error message should mention NaN/Infinity, got: {}",
        err
    );
}

#[tokio::test]
async fn test_neg_inf_poisoning_prevention() {
    let config = HnswConfig {
        dimension: 4,
        ..Default::default()
    };
    let index = HnswIndex::try_new(config).unwrap();

    let tx = TxId(1);
    let id = DocId(1);
    let embedding = vec![f32::NEG_INFINITY, 2.0, 3.0, 4.0];

    let result = index.insert(tx, id, &embedding).await;
    assert!(
        result.is_err(),
        "Insertion of Negative Infinity should fail"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("NaN or Infinity detected"),
        "Error message should mention NaN/Infinity, got: {}",
        err
    );
}
