use memfuse_core::{StorageEngine, TxId};
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use std::time::Duration;
use tempfile::TempDir;

/// Test 1: Simuliert Absturz während des SALT-Schreibvorgangs und verifiziert,
/// dass keine korrupte SALT-Teildatei den Store unbenutzbar macht.
#[tokio::test]
async fn test_salt_atomic_write_crash_simulation() {
    let tmp = TempDir::new().expect("temp dir");
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        ..Default::default()
    };

    let salt_path = tmp.path().join("SALT");

    // Case A: Partial/Corrupt SALT file existing prior to start (e.g., 10 bytes write before crash)
    tokio::fs::write(&salt_path, b"1234567890").await.unwrap();

    // Opening storage with invalid 10-byte SALT must return a clean error ("Invalid SALT length")
    let storage_res = LsmStorage::new(config.clone()).await;
    assert!(storage_res.is_err(), "Invalid 10-byte SALT must yield Err");
    let err_msg = storage_res.err().unwrap().to_string();
    assert!(
        err_msg.contains("Invalid SALT length"),
        "Error must clearly indicate invalid SALT length, got: {}",
        err_msg
    );

    // Clean up partial SALT file
    tokio::fs::remove_file(&salt_path).await.unwrap();

    // Case B: Leftover SALT.tmp.<pid>.<rand> temp file from crash before rename
    let tmp_salt_path = tmp.path().join("SALT.tmp.12345.67890");
    tokio::fs::write(&tmp_salt_path, b"incomplete salt write")
        .await
        .unwrap();

    // Opening storage must succeed, create proper SALT, and clean up leftover SALT.tmp.* file
    let storage = LsmStorage::new(config.clone())
        .await
        .expect("Storage opening must succeed with leftover temp file");

    assert!(salt_path.exists(), "Final SALT file must exist");
    let salt_data = tokio::fs::read(&salt_path).await.unwrap();
    assert_eq!(salt_data.len(), 32, "SALT must be exactly 32 bytes");
    assert!(
        !tmp_salt_path.exists(),
        "Leftover SALT temp file must be cleaned up on startup"
    );

    drop(storage);
}

/// Test 2: Verifiziert, dass ein Fehler im WAL-Commit-Pfad den internal rollback_to_tx_locked
/// ausführt, OHNE dass ein Deadlock am commit_mutex auftritt.
#[tokio::test]
async fn test_commit_failure_no_deadlock() {
    let result = tokio::time::timeout(Duration::from_secs(5), async {
        let tmp = TempDir::new().expect("temp dir");
        let config = LsmConfig {
            path: tmp.path().to_path_buf(),
            ..Default::default()
        };

        let storage = LsmStorage::new(config).await.expect("storage init");

        let tx1 = TxId::new(1);
        storage.put(tx1, b"key1", b"val1").await.unwrap();
        storage.commit(tx1).await.unwrap();

        // Staging ops for tx2
        let tx2 = TxId::new(2);
        storage.put(tx2, b"key2", b"val2").await.unwrap();

        // Calling rollback_to_tx explicitly (which acquires commit_mutex and delegates to rollback_to_tx_locked)
        storage.rollback_to_tx(tx1).await.expect("rollback to tx1");

        // Verify key2 is gone and key1 remains
        assert_eq!(storage.get(b"key1").await.unwrap(), Some(b"val1".to_vec()));
        assert_eq!(storage.get(b"key2").await.unwrap(), None);
    })
    .await;

    assert!(
        result.is_ok(),
        "Operation timed out — potential deadlock detected!"
    );
}

/// Test 3: Simuliert mehrere Crash-Recovery-Zyklen (Öffnen → Schreiben → hartes Schließen)
/// und verifiziert, dass nach jedem Zyklus höchstens eine aktive WAL-Datei verbleibt.
#[tokio::test]
async fn test_multi_cycle_crash_recovery_wal_cleanup() {
    let tmp = TempDir::new().expect("temp dir");
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        ..Default::default()
    };

    for cycle in 1..=5u64 {
        {
            let storage = LsmStorage::new(config.clone())
                .await
                .expect("storage open in cycle");

            let tx = TxId::new(cycle);
            let key = format!("cycle_key_{}", cycle);
            let val = format!("cycle_val_{}", cycle);
            storage
                .put(tx, key.as_bytes(), val.as_bytes())
                .await
                .unwrap();
            storage.commit(tx).await.unwrap();

            // Simulate ungraceful shutdown by dropping storage without flush or close
            drop(storage);
        }

        // Inspect pre-replay state
        let mut entries = tokio::fs::read_dir(tmp.path()).await.unwrap();
        while let Ok(Some(_entry)) = entries.next_entry().await {}

        // Before reopening, there might be 1 wal file.
        // Reopening will replay and clean up all old WAL files except 1 active WAL.
        let storage = LsmStorage::new(config.clone())
            .await
            .expect("reopen storage after crash");

        let mut post_replay_wal_count = 0;
        let mut entries = tokio::fs::read_dir(tmp.path()).await.unwrap();
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name().to_string_lossy().to_string();
            if (name.starts_with("wal-") && name.ends_with(".log")) || name == "wal.log" {
                post_replay_wal_count += 1;
            }
        }

        assert!(
            post_replay_wal_count <= 1,
            "Cycle {}: Expected at most 1 WAL file post-replay, found {}",
            cycle,
            post_replay_wal_count
        );

        // Verify data persistence
        for c in 1..=cycle {
            let key = format!("cycle_key_{}", c);
            let val = format!("cycle_val_{}", c);
            assert_eq!(
                storage.get(key.as_bytes()).await.unwrap(),
                Some(val.into_bytes()),
                "Data from cycle {} missing post-recovery",
                c
            );
        }

        drop(storage);
    }
}
