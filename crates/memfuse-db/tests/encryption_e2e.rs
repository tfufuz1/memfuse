//! End-to-End encryption tests for MemFuse.
// ANCHOR:INTEGRATION:ENCRYPTION-001 STATUS:READY AGENT:12 DATE:2026-05-21

use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use tempfile::TempDir;

#[tokio::test]
async fn test_encryption_at_rest_e2e() {
    let tmp = TempDir::new().expect("Failed to create temp dir");
    let db_path = tmp.path().to_path_buf();
    let passphrase = "correct-passphrase".to_string();

    let config = MemFuseConfig {
        dimension: 3,
        distance_metric: DistanceMetric::Cosine,
        encryption_passphrase: Some(passphrase.clone()),
        ..Default::default()
    };

    // 1. Create encrypted database and insert data
    {
        let db = MemFuse::open_with_config(&db_path, config.clone())
            .await
            .expect("Failed to open DB");

        let col = db
            .collection("secret")
            .await
            .expect("Failed to get collection");
        col.insert(
            "top-secret",
            &[1.0, 1.0, 1.0],
            Some(json!({"data": "highly confidential"})),
        )
        .await
        .expect("Insert failed");

        // Force flush to move data from MemTable to encrypted SSTables
        db.inner_storage()
            .force_flush()
            .await
            .expect("Flush failed");
    }

    // 2. Re-open with correct passphrase
    {
        let db = MemFuse::open_with_config(&db_path, config.clone())
            .await
            .expect("Failed to re-open DB with correct passphrase");

        let col = db
            .collection("secret")
            .await
            .expect("Failed to get collection");
        let doc = col
            .get("top-secret")
            .await
            .expect("Get failed")
            .expect("Doc missing");
        assert_eq!(doc.metadata.unwrap()["data"], "highly confidential");
    }

    // 3. Re-open with incorrect passphrase - Should fail
    {
        let mut wrong_config = config.clone();
        wrong_config.encryption_passphrase = Some("wrong-passphrase".to_string());

        let result = MemFuse::open_with_config(&db_path, wrong_config).await;
        // The error happens during LSM storage initialization when it tries to derive keys and verify existing data
        assert!(result.is_err(), "Opening with wrong passphrase should fail");
    }

    // 4. Re-open with NO passphrase - Should fail
    {
        let mut no_config = config.clone();
        no_config.encryption_passphrase = None;

        let result = MemFuse::open_with_config(&db_path, no_config).await;
        assert!(
            result.is_err(),
            "Opening encrypted DB without passphrase should fail"
        );
    }
}
