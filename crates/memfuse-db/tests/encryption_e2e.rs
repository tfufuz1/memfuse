//! Encryption-at-Rest E2E test.
// ANCHOR:INTEGRATION:E2E-003 STATUS:READY AGENT:12 DATE:2026-05-18

use memfuse_db::{MemFuse, MemFuseConfig};
use serde_json::json;
use tempfile::TempDir;

#[tokio::test]
async fn test_encryption_at_rest_security() {
    let tmp = TempDir::new().expect("failed to create temp dir");
    let path = tmp.path().to_owned();
    let passphrase = "correct-passphrase-123".to_string();

    let config = MemFuseConfig {
        dimension: 3,
        encryption_passphrase: Some(passphrase.clone()),
        ..Default::default()
    };

    // 1. Insert data with encryption
    {
        let db = MemFuse::open_with_config(&path, config.clone())
            .await
            .expect("failed to open encrypted db");
        let col = db
            .collection("secret-col")
            .await
            .expect("collection failed");

        col.insert(
            "secret-doc",
            &[1.0, 2.0, 3.0],
            Some(json!({"data": "classified"})),
        )
        .await
        .expect("encrypted insert failed");

        db.inner_storage()
            .force_flush()
            .await
            .expect("flush failed");
    }

    // 2. Try to open with WRONG passphrase -> Should fail or at least be unable to read data correctly
    {
        let wrong_config = MemFuseConfig {
            dimension: 3,
            encryption_passphrase: Some("wrong-passphrase".to_string()),
            ..Default::default()
        };

        let db_res = MemFuse::open_with_config(&path, wrong_config).await;
        // Depending on implementation, open might succeed but subsequent reads fail,
        // or open might fail during WAL replay.
        if let Ok(db) = db_res {
            let col = db
                .collection("secret-col")
                .await
                .expect("collection failed");
            let res = col.get("secret-doc").await;
            assert!(
                res.is_err(),
                "Reading with wrong passphrase should return error, got {:?}",
                res
            );
        }
    }

    // 3. Open with CORRECT passphrase -> Should succeed
    {
        let db = MemFuse::open_with_config(&path, config)
            .await
            .expect("failed to re-open encrypted db");
        let col = db
            .collection("secret-col")
            .await
            .expect("collection failed");
        let doc = col
            .get("secret-doc")
            .await
            .expect("get failed")
            .expect("doc missing");
        assert_eq!(doc.metadata.unwrap()["data"], "classified");
    }
}
