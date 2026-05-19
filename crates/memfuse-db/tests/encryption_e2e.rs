//! Encryption E2E tests for MemFuse.
// ANCHOR:INTEGRATION:ENCRYPTION-001 STATUS:READY AGENT:12 DATE:2026-05-19

use memfuse_db::{MemFuse, MemFuseConfig};
use serde_json::json;
use tempfile::TempDir;

#[tokio::test]
async fn test_encryption_lifecycle() {
    let tmp = TempDir::new().expect("Failed to create temp dir");
    let path = tmp.path().to_path_buf();
    let passphrase = "correct-horse-battery-staple".to_string();

    // 1. Create an encrypted database
    {
        let config = MemFuseConfig {
            dimension: 3,
            encryption_passphrase: Some(passphrase.clone()),
            ..Default::default()
        };
        let db = MemFuse::open_with_config(&path, config)
            .await
            .expect("Failed to open encrypted DB");

        db.insert(
            "secret-1",
            &[1.0, 2.0, 3.0],
            Some(json!({"msg": "top secret"})),
        )
        .await
        .expect("Failed to insert secret");

        // Use inner_storage() to force flush so we have an SSTable
        db.inner_storage()
            .force_flush()
            .await
            .expect("Failed to flush");
    }

    // 2. Try to open with WRONG passphrase (should fail during open or first operation)
    {
        let config = MemFuseConfig {
            dimension: 3,
            encryption_passphrase: Some("wrong-passphrase".to_string()),
            ..Default::default()
        };
        let result = MemFuse::open_with_config(&path, config).await;

        // Depending on implementation, open might fail or subsequent read might fail.
        // Usually, WAL replay or SSTable open will fail with AEAD error.
        assert!(result.is_err(), "Opening with wrong passphrase should fail");
    }

    // 3. Try to open with NO passphrase (should also fail)
    {
        let config = MemFuseConfig {
            dimension: 3,
            encryption_passphrase: None,
            ..Default::default()
        };
        let result = MemFuse::open_with_config(&path, config).await;
        assert!(
            result.is_err(),
            "Opening encrypted DB without passphrase should fail"
        );
    }

    // 4. Open with CORRECT passphrase and verify data
    {
        let config = MemFuseConfig {
            dimension: 3,
            encryption_passphrase: Some(passphrase),
            ..Default::default()
        };
        let db = MemFuse::open_with_config(&path, config)
            .await
            .expect("Failed to open encrypted DB with correct passphrase");

        let doc = db.get("secret-1").await.expect("get").expect("Doc missing");
        assert_eq!(doc.metadata.unwrap()["msg"], "top secret");
    }
}
