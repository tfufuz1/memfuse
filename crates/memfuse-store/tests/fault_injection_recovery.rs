use memfuse_core::{StorageEngine, TxId};
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use tempfile::TempDir;
use tokio::fs;

#[tokio::test]
async fn test_fault_injection_wal_tail_truncation_recovery() {
    let tmp = TempDir::new().expect("temp dir");
    let path = tmp.path().to_path_buf();

    // 1. Populate storage with committed records
    {
        let config = LsmConfig {
            path: path.clone(),
            ..Default::default()
        };
        let storage = LsmStorage::new(config).await.expect("create storage");

        for i in 1..=5 {
            let tx = TxId::new(i);
            let k = format!("k{}", i).into_bytes();
            let v = format!("v{}", i).into_bytes();
            storage.put(tx, &k, &v).await.expect("put");
            storage.commit(tx).await.expect("commit");
        }
    }

    // 2. Corrupt WAL by truncating partial entry bytes at tail
    let wal_file = path.join("wal.log");
    let data = fs::read(&wal_file).await.expect("read wal");
    assert!(data.len() > 20, "WAL file should contain entries");
    let truncated_data = &data[..data.len() - 15];
    fs::write(&wal_file, truncated_data)
        .await
        .expect("write truncated wal");

    // 3. Re-open LsmStorage; repair/replay should gracefully accept clean entries and ignore truncated tail
    {
        let config = LsmConfig {
            path: path.clone(),
            ..Default::default()
        };
        let storage = LsmStorage::new(config).await.expect("reopen storage");

        // First few entries must be intact
        assert_eq!(
            storage.get(b"k1").await.expect("get k1"),
            Some(b"v1".to_vec())
        );
        assert_eq!(
            storage.get(b"k2").await.expect("get k2"),
            Some(b"v2".to_vec())
        );
    }
}

#[tokio::test]
async fn test_fault_injection_wal_middle_bitflip_detected() {
    let tmp = TempDir::new().expect("temp dir");
    let path = tmp.path().to_path_buf();

    {
        let config = LsmConfig {
            path: path.clone(),
            ..Default::default()
        };
        let storage = LsmStorage::new(config).await.expect("create storage");

        for i in 1..=5 {
            let tx = TxId::new(i);
            let k = format!("k{}", i).into_bytes();
            let v = format!("v{}", i).into_bytes();
            storage.put(tx, &k, &v).await.expect("put");
            storage.commit(tx).await.expect("commit");
        }
    }

    // Flip a bit in the middle of wal.log
    let wal_file = path.join("wal.log");
    let mut data = fs::read(&wal_file).await.expect("read wal");
    let mid_offset = data.len() / 2;
    data[mid_offset] ^= 0xFF;
    fs::write(&wal_file, data)
        .await
        .expect("write corrupted wal");

    // Reopen should detect corruption via CRC or HMAC verification
    let config = LsmConfig {
        path: path.clone(),
        ..Default::default()
    };
    let res = LsmStorage::new(config).await;
    assert!(
        res.is_err(),
        "LsmStorage::new must fail when WAL middle payload is corrupted"
    );
}

#[tokio::test]
async fn test_fault_injection_sstable_temp_files_cleaned_up_on_open() {
    let tmp = TempDir::new().expect("temp dir");
    let path = tmp.path().to_path_buf();

    // Create orphaned .tmp files simulating crash during compaction or SALT creation
    let sst_tmp = path.join("sst-compact-9999.sst.tmp");
    let salt_tmp = path.join("SALT.tmp.1234.5678");

    fs::create_dir_all(&path).await.expect("create dir");
    fs::write(&sst_tmp, b"partial sstable data")
        .await
        .expect("write sst tmp");
    fs::write(&salt_tmp, b"partial salt data")
        .await
        .expect("write salt tmp");

    // Re-open LsmStorage
    let config = LsmConfig {
        path: path.clone(),
        ..Default::default()
    };
    let storage = LsmStorage::new(config).await.expect("open storage");

    // Check orphaned tmp files were removed
    assert!(!sst_tmp.exists(), "Leftover sst.tmp must be removed");
    assert!(!salt_tmp.exists(), "Leftover SALT.tmp must be removed");

    // Verify storage functions normally
    let tx = TxId::new(1);
    storage.put(tx, b"clean_k", b"clean_v").await.expect("put");
    storage.commit(tx).await.expect("commit");
    assert_eq!(
        storage.get(b"clean_k").await.expect("get"),
        Some(b"clean_v".to_vec())
    );
}
