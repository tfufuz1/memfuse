use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use tempfile::TempDir;

#[tokio::test]
async fn test_layer_bounds_enforcement() {
    let tmp = TempDir::new().expect("temp dir"); // #[cfg(test)]
    let config = MemFuseConfig {
        dimension: 1536,
        max_elements: 1000,
        distance_metric: DistanceMetric::Cosine,
        encryption_passphrase: None,
    };

    let db = MemFuse::open_with_config(tmp.path(), config)
        .await
        .expect("open"); // #[cfg(test)]
    let col = db.collection("test").await.expect("col"); // #[cfg(test)]

    // Invalid dimension
    let res = col.insert("bad", &[1.0; 4], None).await;
    assert!(res.is_err());
    assert!(format!("{:?}", res).contains("Dimension mismatch"));
}

#[tokio::test]
async fn test_empty_search_returns_empty_vec() {
    let tmp = TempDir::new().expect("temp dir"); // #[cfg(test)]
    let config = MemFuseConfig {
        dimension: 4,
        max_elements: 1000,
        distance_metric: DistanceMetric::Cosine,
        encryption_passphrase: None,
    };

    let db = MemFuse::open_with_config(tmp.path(), config)
        .await
        .expect("open"); // #[cfg(test)]
    let col = db.collection("empty").await.expect("col"); // #[cfg(test)]

    let results = col.search(&[1.0; 4], 10).await.expect("search"); // #[cfg(test)]
    assert!(results.is_empty());
}
