use memfuse_core::{Result, StorageEngine, TxId};
use memfuse_store::checkpoint::Checkpointer;
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_rollback_durability_across_restarts() -> Result<()> {
    let tmp = TempDir::new().expect("temp dir");
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        ..Default::default()
    };

    let key1 = b"key-durability-1";
    let key2 = b"key-durability-2";

    // 1. Initial State
    {
        let storage = Arc::new(LsmStorage::new(config.clone()).await?);
        let checkpointer = Checkpointer::new(storage.clone());

        let tx1 = TxId::new(1);
        storage.put(tx1, key1, b"val1").await?;
        storage.commit(tx1).await?;
        let cp1 = checkpointer.create_checkpoint(tx1);

        let tx2 = TxId::new(2);
        storage.put(tx2, key2, b"val2").await?;
        storage.commit(tx2).await?;

        // Both keys should be present
        assert_eq!(storage.get(key1).await?, Some(b"val1".to_vec()));
        assert_eq!(storage.get(key2).await?, Some(b"val2".to_vec()));

        // 2. Perform Rollback
        checkpointer.rollback_to(&cp1).await?;
        assert_eq!(storage.get(key1).await?, Some(b"val1".to_vec()));
        assert_eq!(storage.get(key2).await?, None);
    }

    // 3. Restart and verify rolled-back data is STILL GONE (Durable)
    {
        let storage = Arc::new(LsmStorage::new(config.clone()).await?);
        assert_eq!(storage.get(key1).await?, Some(b"val1".to_vec()));
        assert_eq!(
            storage.get(key2).await?,
            None,
            "Data after rollback point survived restart!"
        );

        // Verify we can still write
        let tx3 = TxId::new(3);
        storage.put(tx3, b"key3", b"val3").await?;
        storage.commit(tx3).await?;
        assert_eq!(storage.get(b"key3").await?, Some(b"val3".to_vec()));
    }

    Ok(())
}
