use memfuse_core::{StorageEngine, TxId};
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use std::time::Duration;
use tempfile::TempDir;

#[tokio::test]
async fn test_encrypted_db_roundtrip() {
    let tmp = TempDir::new().expect("temp dir"); // expect #[cfg(test)] // expect #[cfg(test)]
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        memtable_size_limit: 1024,
        max_ram_mb: 64,
        tx_timeout: Duration::from_secs(60),
        encryption_passphrase: Some("super-secret".to_string()),
        ..Default::default()
    };

    // 1. Write data to encrypted DB
    {
        let storage = LsmStorage::new(config.clone())
            .await
            .expect("create storage"); // expect #[cfg(test)]
        let tx = TxId::new(1);
        storage.put(tx, b"key1", b"val1").await.expect("put"); // expect #[cfg(test)]
        storage.commit(tx).await.expect("commit"); // expect #[cfg(test)]

        // Force flush to SSTable
        storage.force_flush().await.expect("flush"); // expect #[cfg(test)]

        let val = storage.get(b"key1").await.expect("get"); // expect #[cfg(test)]
        assert_eq!(val, Some(b"val1".to_vec()));
    }

    // 2. Re-open with same passphrase
    {
        let storage = LsmStorage::new(config.clone())
            .await
            .expect("reopen storage"); // expect #[cfg(test)]
        let val = storage.get(b"key1").await.expect("get after reopen"); // expect #[cfg(test)]
        assert_eq!(val, Some(b"val1".to_vec()));
    }
}

#[tokio::test]
async fn test_wrong_passphrase_fails() {
    let tmp = TempDir::new().expect("temp dir"); // expect #[cfg(test)] // expect #[cfg(test)]
    let mut config = LsmConfig {
        path: tmp.path().to_path_buf(),
        memtable_size_limit: 1024,
        max_ram_mb: 64,
        tx_timeout: Duration::from_secs(60),
        encryption_passphrase: Some("correct-pass".to_string()),
        ..Default::default()
    };

    // 1. Write data
    {
        let storage = LsmStorage::new(config.clone())
            .await
            .expect("create storage"); // expect #[cfg(test)]
        let tx = TxId::new(1);
        storage.put(tx, b"key1", b"val1").await.expect("put"); // expect #[cfg(test)]
        storage.commit(tx).await.expect("commit"); // expect #[cfg(test)]
        storage.force_flush().await.expect("flush"); // expect #[cfg(test)]
    }

    // 2. Re-open with WRONG passphrase
    config.encryption_passphrase = Some("wrong-pass".to_string());
    let result = LsmStorage::new(config).await;

    // Replay or SSTable open should fail
    assert!(result.is_err(), "Opening with wrong passphrase should fail");
}

#[tokio::test]
async fn test_encrypted_db_unreadable_as_plaintext() {
    let tmp = TempDir::new().expect("temp dir"); // expect #[cfg(test)] // expect #[cfg(test)]
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        memtable_size_limit: 1024,
        max_ram_mb: 64,
        tx_timeout: Duration::from_secs(60),
        encryption_passphrase: Some("secret".to_string()),
        ..Default::default()
    };

    // 1. Write known string
    let secret_val = b"THIS_IS_A_SECRET_VALUE_THAT_SHOULD_NOT_BE_IN_PLAINTEXT";
    {
        let storage = LsmStorage::new(config).await.expect("create storage"); // expect #[cfg(test)]
        let tx = TxId::new(1);
        storage.put(tx, b"key1", secret_val).await.expect("put"); // expect #[cfg(test)]
        storage.commit(tx).await.expect("commit"); // expect #[cfg(test)]
        storage.force_flush().await.expect("flush"); // expect #[cfg(test)]
    }

    // 2. Scan files for the secret string
    let mut found = false;
    let mut entries = std::fs::read_dir(tmp.path()).expect("read dir"); // expect #[cfg(test)]
    while let Some(Ok(entry)) = entries.next() {
        if entry.path().is_file() {
            let content = std::fs::read(entry.path()).expect("read file"); // expect #[cfg(test)]
                                                                           // Simple sub-slice search
            if content
                .windows(secret_val.len())
                .any(|window| window == secret_val)
            {
                found = true;
                break;
            }
        }
    }

    assert!(
        !found,
        "Secret value was found in plaintext in the database files!"
    );
}
