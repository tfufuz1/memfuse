use memfuse_core::{Result, StorageEngine, TxId};
use memfuse_store::{LsmConfig, LsmStorage};
use tempfile::tempdir;

#[tokio::test]
async fn test_flush_counter_instance_isolation() -> Result<()> {
    let dir1 = tempdir().map_err(|e| memfuse_core::MemFuseError::InvalidInput(e.to_string()))?;
    let dir2 = tempdir().map_err(|e| memfuse_core::MemFuseError::InvalidInput(e.to_string()))?;

    let config1 = LsmConfig {
        path: dir1.path().to_path_buf(),
        ..Default::default()
    };
    let config2 = LsmConfig {
        path: dir2.path().to_path_buf(),
        ..Default::default()
    };

    let store1 = LsmStorage::new(config1).await?;
    let store2 = LsmStorage::new(config2).await?;

    let tx1 = TxId::new(1);
    store1.put(tx1, b"key1", b"val1").await?;
    store1.commit(tx1).await?;

    let tx2 = TxId::new(1);
    store2.put(tx2, b"key2", b"val2").await?;
    store2.commit(tx2).await?;

    // Force multiple flushes on store1
    store1.force_flush().await?;
    store1.put(TxId::new(2), b"key1_b", b"val1_b").await?;
    store1.commit(TxId::new(2)).await?;
    store1.force_flush().await?;

    // Check WAL files in store1 path
    let mut store1_wals = Vec::new();
    let mut entries1 = tokio::fs::read_dir(dir1.path())
        .await
        .map_err(|e| memfuse_core::MemFuseError::Storage(e.to_string()))?;
    while let Ok(Some(entry)) = entries1.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("wal-") && name.ends_with(".log") {
            store1_wals.push(name);
        }
    }

    // Force single flush on store2
    store2.force_flush().await?;

    let mut store2_wals = Vec::new();
    let mut entries2 = tokio::fs::read_dir(dir2.path())
        .await
        .map_err(|e| memfuse_core::MemFuseError::Storage(e.to_string()))?;
    while let Ok(Some(entry)) = entries2.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("wal-") && name.ends_with(".log") {
            store2_wals.push(name);
        }
    }

    // store1 flushes generated wal-0.log and wal-1.log
    // store2 flush generated wal-0.log independently starting at 0!
    assert!(
        store2_wals.contains(&"wal-0.log".to_string()),
        "Instance 2 flush counter must start at 0 independently of Instance 1 flushes. Found WALs: {:?}",
        store2_wals
    );

    Ok(())
}
