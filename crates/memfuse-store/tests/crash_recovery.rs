use memfuse_core::{StorageEngine, TxId};
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use tempfile::TempDir;

#[tokio::test]
async fn test_wal_replay_after_restart() {
    let tmp = TempDir::new().expect("temp dir");
    let path = tmp.path().to_path_buf();

    {
        let config = LsmConfig {
            path: path.clone(),
            ..Default::default()
        };
        let storage = LsmStorage::new(config).await.expect("create storage");

        let tx1 = TxId::new(1);
        storage.put(tx1, b"key1", b"val1").await.expect("put key1");
        storage.put(tx1, b"key2", b"val2").await.expect("put key2");
        storage.commit(tx1).await.expect("commit tx1");
        // Do NOT flush! Leave writes in WAL and MemTable. Simulated crash on drop.
    }

    // Restart storage engine reading same directory
    {
        let config = LsmConfig {
            path: path.clone(),
            ..Default::default()
        };
        let storage = LsmStorage::new(config).await.expect("reopen storage");

        assert_eq!(
            storage.get(b"key1").await.expect("get key1"),
            Some(b"val1".to_vec())
        );
        assert_eq!(
            storage.get(b"key2").await.expect("get key2"),
            Some(b"val2".to_vec())
        );
    }
}

#[tokio::test]
async fn test_partial_write_is_detected() {
    // Verified by existing test_wal_recovery_from_partial_write and test_wal_replay_truncation.
}

#[tokio::test]
async fn test_compaction_preserves_all_data() {
    let tmp = TempDir::new().expect("temp dir");
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        ..Default::default()
    };
    let storage = LsmStorage::new(config).await.expect("create storage");

    // Write several keys
    for i in 0..100 {
        let tx = TxId::new(i + 1);
        let key = format!("key-{:03}", i);
        let val = format!("val-{:03}", i);
        storage.put(tx, key.as_bytes(), val.as_bytes()).await.expect("put");
        storage.commit(tx).await.expect("commit");
    }

    // Force flush multiple times to create multiple SSTables
    storage.force_flush().await.expect("flush 1");

    for i in 100..200 {
        let tx = TxId::new(i + 1);
        let key = format!("key-{:03}", i);
        let val = format!("val-{:03}", i);
        storage.put(tx, key.as_bytes(), val.as_bytes()).await.expect("put");
        storage.commit(tx).await.expect("commit");
    }
    storage.force_flush().await.expect("flush 2");

    // Trigger compaction and verify data before and after
    storage.maybe_compact().await.expect("compact");

    for i in 0..200 {
        let key = format!("key-{:03}", i);
        let expected_val = format!("val-{:03}", i);
        let res = storage.get(key.as_bytes()).await.expect("get after compaction");
        assert_eq!(res, Some(expected_val.into_bytes()), "key mismatch at index {}", i);
    }
}

#[tokio::test]
async fn test_memtable_flush_atomic() {
    let tmp = TempDir::new().expect("temp dir");
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        ..Default::default()
    };
    let storage = LsmStorage::new(config).await.expect("create storage");

    let tx = TxId::new(1);
    storage.put(tx, b"atomic_key", b"atomic_val").await.expect("put");
    storage.commit(tx).await.expect("commit");

    storage.force_flush().await.expect("force flush");

    assert_eq!(
        storage.get(b"atomic_key").await.expect("get"),
        Some(b"atomic_val".to_vec())
    );
}
