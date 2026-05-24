#[cfg(test)]
mod tests {
    use super::*;
    use crate::LsmConfig;
    use memfuse_core::TxId;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_rollback_to_checkpoint() {
        let tmp = TempDir::new().unwrap(); // unwrap allowed
        let mut lsm_config = LsmConfig::default();
        lsm_config.path = tmp.path().to_path_buf();

        let storage = LsmStorage::new(lsm_config).await.unwrap(); // unwrap allowed

        let tx1 = TxId::new(1);
        storage.put(tx1, b"key1", b"val1").await.unwrap(); // unwrap allowed
        storage.commit(tx1).await.unwrap(); // unwrap allowed

        // 1. Create checkpoint v1
        storage.create_checkpoint("v1").await.unwrap(); // unwrap allowed

        // 2. Add more data
        let tx2 = TxId::new(2);
        storage.put(tx2, b"key2", b"val2").await.unwrap(); // unwrap allowed
        storage.commit(tx2).await.unwrap(); // unwrap allowed

        {
            let val = storage.get(b"key1").await.unwrap(); // unwrap allowed
            assert_eq!(val, Some(b"val1".to_vec()));
            let val = storage.get(b"key2").await.unwrap(); // unwrap allowed
            assert_eq!(val, Some(b"val2".to_vec()));
        }

        // 3. Rollback to v1
        storage.rollback_to_checkpoint("v1").await.unwrap(); // unwrap allowed

        // 4. Verify state
        {
            let val = storage.get(b"key1").await.unwrap(); // unwrap allowed
            assert_eq!(val, Some(b"val1".to_vec()));
            let val = storage.get(b"key2").await.unwrap(); // unwrap allowed
            assert_eq!(val, None); // Should be gone!
        }

        // 5. Verify we can still write and seq_no is correct
        let tx3 = TxId::new(3);
        storage.put(tx3, b"key3", b"val3").await.unwrap(); // unwrap allowed
        storage.commit(tx3).await.unwrap(); // unwrap allowed
        {
            let val = storage.get(b"key3").await.unwrap(); // unwrap allowed
            assert_eq!(val, Some(b"val3".to_vec()));
        }
    }
}
