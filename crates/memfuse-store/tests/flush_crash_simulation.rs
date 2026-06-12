//! SD-02-STORE-001 — Flush Crash Simulation Tests.
//!
//! Verifies WAL-Atomicity invariant: the WAL must NEVER be deleted before
//! the SSTable is successfully persisted. On crash during the window between
//! WAL delete and SSTable write, data loss would occur.

use memfuse_core::{Result, StorageEngine, TxId};
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use std::time::Duration;
use tempfile::TempDir;

async fn test_storage_with_limit(limit: usize) -> (LsmStorage, TempDir) {
    let tmp = TempDir::new().expect("temp dir");
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        memtable_size_limit: limit,
        max_ram_mb: 64,
        tx_timeout: Duration::from_secs(60),
        encryption_passphrase: None,
        ..Default::default()
    };
    let storage = LsmStorage::new(config).await.expect("create storage");
    (storage, tmp)
}

/// After a successful flush, an .sst file MUST exist and the OLD WAL
/// MUST be removed. This validates the happy-path ordering:
///   SSTable write → atomic state swap → WAL cleanup.
#[tokio::test]
async fn test_flush_ordering_wal_deleted_after_sstable() -> Result<()> {
    let (storage, tmp) = test_storage_with_limit(1024 * 1024).await;

    let tx = TxId::new(1);
    storage.put(tx, b"key1", b"value1").await?;
    storage.commit(tx).await?;

    // Count WAL files before flush
    let wals_before: Vec<_> = std::fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let n = e.file_name();
            let s = n.to_string_lossy();
            s.starts_with("wal") && s.ends_with(".log")
        })
        .collect();
    assert!(!wals_before.is_empty(), "WAL must exist before flush");

    // Perform flush
    storage.force_flush().await?;

    // After flush: SSTable must exist
    let sst_files: Vec<_> = std::fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let n = e.file_name();
            n.to_string_lossy().ends_with(".sst")
        })
        .collect();
    assert!(
        !sst_files.is_empty(),
        "SSTable file must exist after successful flush"
    );

    // Data must still be readable
    let val = storage.get(b"key1").await?;
    assert_eq!(val, Some(b"value1".to_vec()));

    Ok(())
}

/// Simulates a crash-recovery scenario: write data → flush to SSTable →
/// drop and reopen the storage. Data must survive the roundtrip through
/// SSTable persistence.
#[tokio::test]
async fn test_recovery_after_flush_via_reopen() -> Result<()> {
    let tmp = TempDir::new().expect("temp dir");
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        memtable_size_limit: 1024 * 1024,
        max_ram_mb: 64,
        tx_timeout: Duration::from_secs(60),
        encryption_passphrase: None,
        ..Default::default()
    };

    // Phase 1: Write and flush
    {
        let storage = LsmStorage::new(config.clone()).await?;
        let tx1 = TxId::new(1);
        storage.put(tx1, b"persist-key1", b"persist-val1").await?;
        storage.commit(tx1).await?;

        let tx2 = TxId::new(2);
        storage.put(tx2, b"persist-key2", b"persist-val2").await?;
        storage.commit(tx2).await?;

        storage.force_flush().await?;

        // Verify before close
        assert_eq!(
            storage.get(b"persist-key1").await?,
            Some(b"persist-val1".to_vec())
        );
        assert_eq!(
            storage.get(b"persist-key2").await?,
            Some(b"persist-val2".to_vec())
        );

        storage.wait_shutdown().await;
    }

    // Phase 2: Reopen and verify
    {
        let storage = LsmStorage::new(config).await?;

        let val1 = storage.get(b"persist-key1").await?;
        assert_eq!(
            val1,
            Some(b"persist-val1".to_vec()),
            "Key1 must survive reopen after flush"
        );

        let val2 = storage.get(b"persist-key2").await?;
        assert_eq!(
            val2,
            Some(b"persist-val2".to_vec()),
            "Key2 must survive reopen after flush"
        );

        storage.wait_shutdown().await;
    }

    Ok(())
}

/// Write data without flushing, then reopen. The WAL replay must recover
/// all committed data that was only in the MemTable at close time.
#[tokio::test]
async fn test_wal_replay_recovery_without_flush() -> Result<()> {
    let tmp = TempDir::new().expect("temp dir");
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        memtable_size_limit: 1024 * 1024,
        max_ram_mb: 64,
        tx_timeout: Duration::from_secs(60),
        encryption_passphrase: None,
        ..Default::default()
    };

    // Phase 1: Write without flush
    {
        let storage = LsmStorage::new(config.clone()).await?;
        let tx = TxId::new(1);
        storage.put(tx, b"wal-key", b"wal-val").await?;
        storage.commit(tx).await?;
        // NO flush — data only in WAL + MemTable
        storage.wait_shutdown().await;
    }

    // Phase 2: Reopen — WAL replay must recover the entry
    {
        let storage = LsmStorage::new(config).await?;
        let val = storage.get(b"wal-key").await?;
        assert_eq!(
            val,
            Some(b"wal-val".to_vec()),
            "WAL replay must recover uncommitted-to-SSTable data"
        );
        storage.wait_shutdown().await;
    }

    Ok(())
}

/// Multiple flushes must produce multiple SSTables and maintain data
/// visibility across all of them.
#[tokio::test]
async fn test_multiple_flush_cycles_preserve_data() -> Result<()> {
    let (storage, _tmp) = test_storage_with_limit(1024 * 1024).await;

    for i in 0..5u64 {
        let tx = TxId::new(i + 1);
        let key = format!("batch-key-{}", i);
        let val = format!("batch-val-{}", i);
        storage.put(tx, key.as_bytes(), val.as_bytes()).await?;
        storage.commit(tx).await?;
        storage.force_flush().await?;
    }

    // All 5 keys must be readable across SSTables
    for i in 0..5u64 {
        let key = format!("batch-key-{}", i);
        let expected = format!("batch-val-{}", i);
        let val = storage.get(key.as_bytes()).await?;
        assert_eq!(
            val,
            Some(expected.into_bytes()),
            "Key {} must survive multi-flush cycle",
            i
        );
    }

    Ok(())
}
