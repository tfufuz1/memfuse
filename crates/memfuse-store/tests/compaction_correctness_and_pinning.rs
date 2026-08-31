use memfuse_core::{Result, StorageEngine, TxId};
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use tempfile::TempDir;

#[tokio::test]
async fn test_multigenerational_overwrites_and_tombstones() -> Result<()> {
    let tmp = TempDir::new().expect("temp dir");
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        ..Default::default()
    };
    let storage = LsmStorage::new(config).await?;

    let tx1 = TxId::new(1);
    storage.put(tx1, b"key1", b"v1").await?;
    storage.put(tx1, b"key2", b"v1").await?;
    storage.commit(tx1).await?;
    storage.force_flush().await?;

    let tx2 = TxId::new(2);
    storage.put(tx2, b"key1", b"v2").await?;
    storage.delete(tx2, b"key2").await?;
    storage.commit(tx2).await?;
    storage.force_flush().await?;

    assert_eq!(
        storage.get(b"key1").await?,
        Some(b"v2".to_vec()),
        "Newer version v2 in SSTable 2 must shadow v1 in SSTable 1"
    );
    assert_eq!(
        storage.get(b"key2").await?,
        None,
        "Tombstone in SSTable 2 must mask value in SSTable 1"
    );

    let tx3 = TxId::new(3);
    storage.put(tx3, b"key1", b"v3").await?;
    storage.commit(tx3).await?;
    storage.force_flush().await?;

    assert_eq!(storage.get(b"key1").await?, Some(b"v3".to_vec()));

    Ok(())
}

#[tokio::test]
async fn test_compaction_gc_unpinned_vs_pinned() -> Result<()> {
    let tmp = TempDir::new().expect("temp dir");
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        ..Default::default()
    };
    let storage = LsmStorage::new(config).await?;

    let tx1 = TxId::new(1);
    storage.put(tx1, b"keyA", b"valA").await?;
    storage.commit(tx1).await?;
    storage.force_flush().await?;

    let tx2 = TxId::new(2);
    storage.put(tx2, b"keyB", b"valB").await?;
    storage.commit(tx2).await?;
    storage.force_flush().await?;

    storage.pin_checkpoint(2).await?;

    let tx3 = TxId::new(3);
    storage.delete(tx3, b"keyA").await?;
    storage.commit(tx3).await?;
    storage.force_flush().await?;

    storage.maybe_compact().await?;

    assert_eq!(storage.get(b"keyA").await?, None);
    assert_eq!(storage.get(b"keyB").await?, Some(b"valB".to_vec()));

    let key_a_at_tx2 = storage.get_at_seq(b"keyA", 2).await?;
    assert_eq!(
        key_a_at_tx2,
        Some(b"valA".to_vec()),
        "Pinned snapshot must preserve historical version during compaction"
    );

    storage.unpin_checkpoint(2).await?;
    storage.maybe_compact().await?;

    assert_eq!(storage.get(b"keyA").await?, None);

    Ok(())
}
