use memfuse_core::{Result, StorageEngine, TxId};
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use tempfile::tempdir;
use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;

#[tokio::test]
async fn test_flush_durability_on_failure() -> Result<()> {
    let dir = tempdir().unwrap();
    let config = LsmConfig {
        path: dir.path().to_path_buf(),
        ..Default::default()
    };
    let storage = LsmStorage::new(config).await.unwrap();

    // 1. Put data to ensure WAL has content
    let tx = TxId::new(1);
    storage.put(tx, b"key1", b"val1").await.unwrap();
    storage.commit(tx).await.unwrap();
    
    // 2. Find the WAL file
    let entries: Vec<_> = std::fs::read_dir(dir.path()).unwrap().filter_map(|e| e.ok()).collect();
    
    let wal_path = entries
        .iter()
        .find(|e| {
            let name = e.file_name();
            let name_str = name.to_string_lossy();
            name_str.starts_with("wal") && name_str.ends_with(".log")
        })
        .expect("WAL file should exist")
        .path();


    assert!(wal_path.exists(), "WAL file must exist before flush");

    // 3. Make directory read-only to force SstableBuilder::create to fail
    // We give read and execute (to list), but NO write.
    std::fs::set_permissions(dir.path(), Permissions::from_mode(0o555)).unwrap();

    // 4. Trigger flush. It should fail because it cannot create the SSTable file.
    let flush_result = storage.force_flush().await;
    assert!(flush_result.is_err(), "Flush must fail when directory is read-only");

    // 5. CRITICAL CHECK: Does the WAL file still exist?
    // In the BROKEN version, it was deleted before the SSTable was created.
    let wal_exists = wal_path.exists();
    
    // Restore permissions so tempdir can cleanup
    let _ = std::fs::set_permissions(dir.path(), Permissions::from_mode(0o755));

    assert!(wal_exists, "WAL file MUST still exist if flush failed to persist SSTable");

    Ok(())
}
