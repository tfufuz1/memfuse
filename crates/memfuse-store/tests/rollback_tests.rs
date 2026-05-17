use memfuse_core::{StorageEngine, TxId};
use memfuse_store::{Checkpointer, CompactionConfig, LsmConfig, LsmStorage};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

async fn setup_storage() -> (Arc<LsmStorage>, TempDir) {
    let tmp = TempDir::new().expect("temp dir");
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        memtable_size_limit: 1024 * 1024,
        max_ram_mb: 64,
        tx_timeout: Duration::from_secs(60),
        compaction: CompactionConfig::default(),
        encryption_passphrase: None,
    };
    let storage = Arc::new(LsmStorage::new(config).await.expect("create storage"));
    (storage, tmp)
}

#[tokio::test]
async fn test_time_travel_rollback() {
    let (storage, _tmp) = setup_storage().await;
    let checkpointer = Checkpointer::new(Arc::clone(&storage));

    // 1. Initial State (TX 1)
    let tx1 = TxId::new(1);
    storage.put(tx1, b"key1", b"val1").await.expect("put1");
    storage.commit(tx1).await.expect("commit1");

    // 2. State at Checkpoint (TX 2)
    let tx2 = TxId::new(2);
    storage.put(tx2, b"key2", b"val2").await.expect("put2");
    storage.commit(tx2).await.expect("commit2");

    let checkpoint = checkpointer.create_checkpoint(tx2);
    assert_eq!(checkpoint.tx_id, tx2);

    // 3. More Writes (TX 3, TX 4)
    let tx3 = TxId::new(3);
    storage
        .put(tx3, b"key1", b"val1_updated")
        .await
        .expect("put3");
    storage.commit(tx3).await.expect("commit3");

    let tx4 = TxId::new(4);
    storage.delete(tx4, b"key2").await.expect("delete4");
    storage.commit(tx4).await.expect("commit4");

    // Verify current state
    assert_eq!(
        storage.get(b"key1").await.unwrap(),
        Some(b"val1_updated".to_vec())
    );
    assert_eq!(storage.get(b"key2").await.unwrap(), None);

    // 4. Rollback to TX 2
    checkpointer
        .rollback_to(&checkpoint)
        .await
        .expect("rollback");

    // Verify rolled back state
    assert_eq!(storage.get(b"key1").await.unwrap(), Some(b"val1".to_vec()));
    assert_eq!(storage.get(b"key2").await.unwrap(), Some(b"val2".to_vec()));

    // 5. Verify next sequence number (it should continue after the replayed max_seq)
    let tx5 = TxId::new(5);
    storage.put(tx5, b"key3", b"val3").await.expect("put5");
    storage.commit(tx5).await.expect("commit5");
    assert_eq!(storage.get(b"key3").await.unwrap(), Some(b"val3".to_vec()));
}

#[tokio::test]
async fn test_rollback_across_flushes() {
    let tmp = TempDir::new().expect("temp dir");
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        memtable_size_limit: 100, // Small limit
        max_ram_mb: 64,
        tx_timeout: Duration::from_secs(60),
        compaction: CompactionConfig::default(),
        encryption_passphrase: None,
    };
    let storage = Arc::new(LsmStorage::new(config).await.expect("create storage"));
    let checkpointer = Checkpointer::new(Arc::clone(&storage));

    // TX 1: Goes to SSTable 1
    let tx1 = TxId::new(1);
    storage
        .put(tx1, b"key_stable", b"value_stable")
        .await
        .expect("put1");
    storage.commit(tx1).await.expect("commit1");
    storage.flush().await.expect("flush1");

    // TX 2: Goes to MemTable (Checkpoint here)
    let tx2 = TxId::new(2);
    storage
        .put(tx2, b"key_volatile", b"value_volatile")
        .await
        .expect("put2");
    storage.commit(tx2).await.expect("commit2");
    let checkpoint = checkpointer.create_checkpoint(tx2);

    // TX 3: Overwrites
    let tx3 = TxId::new(3);
    storage
        .put(tx3, b"key_stable", b"value_broken")
        .await
        .expect("put3");
    storage.commit(tx3).await.expect("commit3");

    // Rollback
    checkpointer
        .rollback_to(&checkpoint)
        .await
        .expect("rollback");

    // Verify
    assert_eq!(
        storage.get(b"key_stable").await.unwrap(),
        Some(b"value_stable".to_vec())
    );
    assert_eq!(
        storage.get(b"key_volatile").await.unwrap(),
        Some(b"value_volatile".to_vec())
    );
}
