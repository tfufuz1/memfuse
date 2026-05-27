#[cfg(test)]
mod tests {
    use super::*;
    use crate::LsmConfig;
    use crate::LsmStorage;
    use memfuse_core::{StorageEngine, TxId};
    use std::sync::Arc;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_rollback_to_checkpoint() {
        let tmp = TempDir::new().unwrap() // unwrap allowed (AGENT:09)
        ;
        let config = LsmConfig {
            path: tmp.path().to_path_buf(),
            ..Default::default()
        };
        let storage = LsmStorage::new(config).await.unwrap() // unwrap allowed (AGENT:09)
        ;

        // 1. Write some data
        let tx1 = TxId::new(1);
        storage.put(tx1, b"key1", b"val1").await.unwrap() // unwrap allowed (AGENT:09)
        ;
        storage.commit(tx1).await.unwrap() // unwrap allowed (AGENT:09)
        ;

        // 2. Create checkpoint
        storage.pin_checkpoint(1).await.unwrap() // unwrap allowed (AGENT:09)
        ;

        // 3. Write more data and rollback
        let tx2 = TxId::new(2);
        storage.put(tx2, b"key2", b"val2").await.unwrap() // unwrap allowed (AGENT:09)
        ;
        storage.commit(tx2).await.unwrap() // unwrap allowed (AGENT:09)
        ;

        assert_eq!(
            storage.get(b"key1").await.unwrap(), // unwrap allowed (AGENT:09)
            Some(b"val1".to_vec())
        );
        assert_eq!(
            storage.get(b"key2").await.unwrap(), // unwrap allowed (AGENT:09)
            Some(b"val2".to_vec())
        );

        // Simulated crash/reopen at checkpoint 1
        drop(storage);

        let config2 = LsmConfig {
            path: tmp.path().to_path_buf(),
            ..Default::default()
        };
        let storage = LsmStorage::new(config2).await.unwrap() // unwrap allowed (AGENT:09)
        ;

        // 4. Verify state
        assert_eq!(
            storage.get(b"key1").await.unwrap(), // unwrap allowed (AGENT:09)
            Some(b"val1".to_vec())
        );
        assert_eq!(storage.get(b"key2").await.unwrap(), None); // Should be gone! // unwrap allowed (AGENT:09)

        // 5. Verify we can still write and seq_no is correct
        let tx3 = TxId::new(3);
        storage.put(tx3, b"key3", b"val3").await.unwrap() // unwrap allowed (AGENT:09)
        ;
        storage.commit(tx3).await.unwrap() // unwrap allowed (AGENT:09)
        ;
        assert_eq!(
            storage.get(b"key3").await.unwrap(), // unwrap allowed (AGENT:09)
            Some(b"val3".to_vec())
        );
    }
}
