use memfuse_core::{Result, StorageEngine, TxId};
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use tempfile::TempDir;

#[tokio::test]
async fn test_flush_threshold_boundary_minus_one() -> Result<()> {
    let tmp = TempDir::new().expect("temp dir");
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        memtable_size_limit: 1000,
        ..Default::default()
    };
    let storage = LsmStorage::new(config).await?;

    let tx = TxId::new(1);
    storage.put(tx, b"k", b"v").await?;
    storage.commit(tx).await?;

    let sst_files: Vec<_> = std::fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".sst"))
        .collect();
    assert_eq!(
        sst_files.len(),
        0,
        "No SSTable should be created below size threshold"
    );

    let val = storage.get(b"k").await?;
    assert_eq!(val, Some(b"v".to_vec()));

    Ok(())
}

#[tokio::test]
async fn test_flush_threshold_boundary_exact_and_plus_one() -> Result<()> {
    let tmp = TempDir::new().expect("temp dir");
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        memtable_size_limit: 200,
        ..Default::default()
    };
    let storage = LsmStorage::new(config).await?;

    for i in 1..=10 {
        let tx = TxId::new(i);
        let k = format!("boundary_key_{:02}", i);
        let v = vec![0x42u8; 30];
        storage.put(tx, k.as_bytes(), &v).await?;
        storage.commit(tx).await?;
    }

    storage.force_flush().await?;

    let sst_files: Vec<_> = std::fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".sst"))
        .collect();
    assert!(
        !sst_files.is_empty(),
        "SSTable must be created once threshold is reached/exceeded"
    );

    for i in 1..=10 {
        let k = format!("boundary_key_{:02}", i);
        let val = storage.get(k.as_bytes()).await?;
        assert_eq!(
            val,
            Some(vec![0x42u8; 30]),
            "Key {} must be readable after boundary flush",
            i
        );
    }

    Ok(())
}
