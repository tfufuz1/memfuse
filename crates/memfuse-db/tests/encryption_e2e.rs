//! End-to-end encryption tests for MemFuse.
// ANCHOR:INTEGRATION:ENCRYPTION-E2E STATUS:READY AGENT:12 DATE:2026-05-20

use memfuse_db::{MemFuse, MemFuseConfig};
use serde_json::json;
use tempfile::TempDir;

#[tokio::test]
async fn test_encryption_at_rest_full_stack() {
    let tmp = TempDir::new().expect("Failed to create temp dir");
    let db_path = tmp.path().to_path_buf();
    let passphrase = "my-secure-passphrase".to_string();

    let config = MemFuseConfig {
        dimension: 3,
        encryption_passphrase: Some(passphrase.clone()),
        ..Default::default()
    };

    // 1. Create encrypted DB and insert sensitive data
    {
        let db = MemFuse::open_with_config(&db_path, config.clone())
            .await
            .expect("Failed to open encrypted DB");

        db.insert(
            "secret-doc",
            &[1.0, 0.0, 0.0],
            Some(json!({"secret": "Top secret content"})),
        )
        .await
        .expect("Insert failed");

        // Force flush to ensure it's on disk (and thus encrypted)
        db.inner_storage().force_flush().await.expect("Flush failed");
    }

    // 2. Attempt to open WITHOUT passphrase (should fail or be unable to read)
    {
        let wrong_config = MemFuseConfig {
            dimension: 3,
            encryption_passphrase: None,
            ..Default::default()
        };

        // Opening might succeed (LSM doesn't strictly check on open yet)
        // but reading MUST fail or return garbage/error.
        let db_res = MemFuse::open_with_config(&db_path, wrong_config).await;

        match db_res {
            Ok(db) => {
                // If it opened, trying to get the secret doc should fail because WAL replay or SSTable read fails
                let res = db.get("secret-doc").await;
                assert!(
                    res.is_err(),
                    "Reading encrypted data without passphrase should fail"
                );
            }
            Err(_) => {
                // Fails early on open (e.g. during WAL replay) - also a success for this test
            }
        }
    }

    // 3. Attempt to open with WRONG passphrase
    {
        let wrong_config = MemFuseConfig {
            dimension: 3,
            encryption_passphrase: Some("wrong-passphrase".to_string()),
            ..Default::default()
        };

        let db_res = MemFuse::open_with_config(&db_path, wrong_config).await;

        // WAL replay with wrong key usually fails due to integrity check or decryption error
        assert!(db_res.is_err(), "Opening with wrong passphrase should fail");
    }

    // 4. Open with CORRECT passphrase and verify
    {
        let db = MemFuse::open_with_config(&db_path, config)
            .await
            .expect("Failed to re-open with correct passphrase");

        let doc = db.get("secret-doc").await.expect("Get failed").expect("Not found");
        assert_eq!(doc.metadata.unwrap()["secret"], "Top secret content");

        let search = db.search(&[1.0, 0.0, 0.0], 1).await.expect("Search failed");
        assert_eq!(search[0].id, "secret-doc");
    }
}

#[tokio::test]
async fn test_encryption_migration_denied() {
    // Test that you can't open a non-encrypted DB as encrypted and vice versa
    let tmp = TempDir::new().expect("temp dir");
    let db_path = tmp.path().to_path_buf();

    // 1. Create plain DB
    {
        let config = MemFuseConfig {
            dimension: 3,
            ..Default::default()
        };
        let db = MemFuse::open_with_config(&db_path, config).await.unwrap();
        db.insert("plain", &[1.0, 0.0, 0.0], None).await.unwrap();
    }

    // 2. Try to open as encrypted
    let enc_config = MemFuseConfig {
        dimension: 3,
        encryption_passphrase: Some("secret".to_string()),
        ..Default::default()
    };
    let db_res = MemFuse::open_with_config(&db_path, enc_config).await;

    // This should fail because it tries to replay/read plain WAL/SST with a key
    assert!(db_res.is_err(), "Opening plain DB as encrypted should fail");
}
