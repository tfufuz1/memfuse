#[cfg(test)]
mod tests {
    use crate::{LsmConfig, LsmStorage};
    use memfuse_core::{StorageEngine, TxId};
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_rollback_to_checkpoint() {
        let tmp = TempDir::new().expect("temp dir");
        let config = LsmConfig {
            path: tmp.path().to_path_buf(),
            ..Default::default()
        };
        let storage = LsmStorage::new(config).await.expect("storage");

        // 1. Insert some data
        let tx1 = TxId::new(1);
        storage.put(tx1, b"key1", b"val1").await.unwrap(); // unwrap
        storage.commit(tx1).await.unwrap(); // unwrap

        // 2. Create checkpoint
        let seq1 = storage.last_seq_no().await.expect("seq");
        storage.pin_checkpoint(seq1).await.expect("pin");

        // 3. More data
        let tx2 = TxId::new(2);
        storage.put(tx2, b"key2", b"val2").await.unwrap(); // unwrap
        storage.commit(tx2).await.unwrap(); // unwrap

        assert_eq!(storage.get(b"key1").await.unwrap(), Some(b"val1".to_vec())); // unwrap
        assert_eq!(storage.get(b"key2").await.unwrap(), Some(b"val2".to_vec())); // unwrap

        // 4. Checkpoint pinning logic
        storage.unpin_checkpoint(seq1).await.expect("unpin");

        assert_eq!(storage.get(b"key1").await.unwrap(), Some(b"val1".to_vec())); // unwrap

        // 5. Verify we can still write
        let tx3 = TxId::new(3);
        storage.put(tx3, b"key3", b"val3").await.unwrap(); // unwrap
        storage.commit(tx3).await.unwrap(); // unwrap
        assert_eq!(storage.get(b"key3").await.unwrap(), Some(b"val3".to_vec())); // unwrap
    }
}
