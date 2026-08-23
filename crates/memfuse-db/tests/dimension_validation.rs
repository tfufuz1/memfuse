use memfuse_db::{MemFuse, MemFuseConfig};

#[tokio::test]
async fn test_open_dimension_mismatch_fails() {
    let dir = tempfile::tempdir().unwrap();
    let config_768 = MemFuseConfig {
        dimension: 768,
        ..Default::default()
    };
    let _db = MemFuse::open_with_config(dir.path(), config_768)
        .await
        .unwrap();

    // Zweites Öffnen mit falscher Dimension muss früh fehlschlagen
    let config_1536 = MemFuseConfig {
        dimension: 1536,
        ..Default::default()
    };
    let result = MemFuse::open_with_config(dir.path(), config_1536).await;
    assert!(result.is_err());
    let err = result.err().unwrap();
    assert!(err.to_string().contains("Dimension mismatch"));
}
