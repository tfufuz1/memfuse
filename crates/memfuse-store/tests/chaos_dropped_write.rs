// FILE-CONTEXT: Chaos test for fsync/I/O error propagation and durability guarantees during WAL write drops. (TS: 2026-08-30) (SESSION: 283abf0f)
//! Chaos test proving fsync/I/O error propagation and non-corruption invariants in `LsmStorage`.
//!
//! Evaluates the "fsync Error Propagation (ABSOLUT)" invariant defined in `crates/memfuse-store/AGENTS.md`.

use memfuse_core::{MemFuseError, StorageEngine, TxId};
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use std::fs::Permissions;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;

#[cfg(unix)]
extern "C" {
    fn open(path: *const std::ffi::c_char, flags: std::ffi::c_int) -> std::ffi::c_int;
    fn dup2(oldfd: std::ffi::c_int, newfd: std::ffi::c_int) -> std::ffi::c_int;
    fn close(fd: std::ffi::c_int) -> std::ffi::c_int;
}

/// Verifies that an I/O failure during `commit()`/`append()` properly propagates an error,
/// leaves `last_committed_tx` unchanged, allows recovery once write permissions are restored,
/// and preserves previously committed entries without collateral damage.
///
/// # Platform Restriction
/// Active file descriptor manipulation via `/proc/self/fd` and file mode permissions (`0o444`)
/// is target-scoped to Unix platforms (`#[cfg(unix)]`). Under POSIX rules, file permissions
/// are evaluated at `open()` time; simulating a mid-flight write/fsync drop on an open file description
/// requires replacing the descriptor via `/proc/self/fd`.
#[tokio::test]
#[cfg(unix)]
async fn test_chaos_dropped_write_error_propagation_and_recovery() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().to_path_buf();

    let config = LsmConfig {
        path: db_path.clone(),
        ..Default::default()
    };

    let storage = LsmStorage::new(config).await.expect("Storage creation must succeed");

    // 1. Commit initial entry successfully
    let tx1 = TxId::new(1);
    storage.put(tx1, b"key1", b"val1").await.expect("put tx1 must succeed");
    storage.commit(tx1).await.expect("commit tx1 must succeed");

    let last_tx_1 = storage.last_tx_id().await.expect("last_tx_id query must succeed");
    assert_eq!(
        last_tx_1, tx1,
        "last_committed_tx must be updated after initial successful commit"
    );

    // Verify key1 is readable
    let val1 = storage.get(b"key1").await.expect("get key1 must succeed");
    assert_eq!(
        val1,
        Some(b"val1".to_vec()),
        "key1 must be readable before error injection"
    );

    // 2. Set active WAL file permissions to read-only (0o444)
    // and replace open file description for `wal.log` with a read-only descriptor.
    let wal_path = db_path.join("wal.log");
    let canon_wal_path = std::fs::canonicalize(&wal_path).unwrap_or_else(|_| wal_path.clone());
    std::fs::set_permissions(&wal_path, Permissions::from_mode(0o444)).expect("set_permissions 0o444 must succeed");

    // Find open file descriptors pointing to wal_path in /proc/self/fd and replace them with O_RDONLY
    if let Ok(entries) = std::fs::read_dir("/proc/self/fd") {
        for entry in entries.flatten() {
            if let Ok(target) = std::fs::read_link(entry.path()) {
                let canon_target = std::fs::canonicalize(&target).unwrap_or(target.clone());
                if target == wal_path || target == canon_wal_path || canon_target == canon_wal_path {
                    if let Ok(fd_num) = entry.file_name().to_string_lossy().parse::<i32>() {
                        use std::os::unix::ffi::OsStrExt;
                        let path_c = std::ffi::CString::new(canon_wal_path.as_os_str().as_bytes()).expect("CString conversion must succeed");
                        unsafe {
                            let ro_fd = open(path_c.as_ptr(), 0); // O_RDONLY = 0
                            if ro_fd >= 0 {
                                dup2(ro_fd, fd_num);
                                close(ro_fd);
                            }
                        }
                    }
                }
            }
        }
    }

    // 3. Attempt a second commit (tx2).
    // Expectation: Err(MemFuseError::Storage(_)) or Err(MemFuseError::Io(_))
    let tx2 = TxId::new(2);
    storage.put(tx2, b"key2", b"val2").await.expect("put tx2 must succeed");

    let commit_result = storage.commit(tx2).await;

    // Verify that commit failed with the expected error variant WITHOUT calling .unwrap()
    match &commit_result {
        Err(MemFuseError::Storage(msg)) => {
            assert!(
                msg.contains("WAL batch")
                    || msg.contains("Bad file descriptor")
                    || msg.contains("Permission denied")
                    || msg.contains("Commit failed"),
                "Expected I/O or WAL error in MemFuseError::Storage, got: {msg}"
            );
        }
        Err(MemFuseError::Io(io_err)) => {
            assert!(
                io_err.kind() == std::io::ErrorKind::PermissionDenied
                    || io_err.kind() == std::io::ErrorKind::Other
                    || io_err.raw_os_error() == Some(9), // EBADF
                "Expected EBADF or PermissionDenied I/O error, got: {:?}",
                io_err
            );
        }
        Err(other) => {
            panic!(
                "Expected MemFuseError::Storage or MemFuseError::Io on write failure, got: {:?}",
                other
            );
        }
        Ok(()) => {
            panic!("Mutation survival check: commit() MUST NOT succeed when WAL file is read-only!");
        }
    }

    // 4. Restore write permissions to file (0o644)
    std::fs::set_permissions(&wal_path, Permissions::from_mode(0o644)).expect("set_permissions 0o644 must succeed");

    // Re-open WAL file handle for wal_path in /proc/self/fd with O_RDWR | O_APPEND
    if let Ok(entries) = std::fs::read_dir("/proc/self/fd") {
        for entry in entries.flatten() {
            if let Ok(target) = std::fs::read_link(entry.path()) {
                let canon_target = std::fs::canonicalize(&target).unwrap_or(target.clone());
                if target == wal_path || target == canon_wal_path || canon_target == canon_wal_path {
                    if let Ok(fd_num) = entry.file_name().to_string_lossy().parse::<i32>() {
                        use std::os::unix::ffi::OsStrExt;
                        let path_c = std::ffi::CString::new(canon_wal_path.as_os_str().as_bytes()).expect("CString conversion must succeed");
                        unsafe {
                            let rw_fd = open(path_c.as_ptr(), 2 | 1024); // O_RDWR | O_APPEND
                            if rw_fd >= 0 {
                                dup2(rw_fd, fd_num);
                                close(rw_fd);
                            }
                        }
                    }
                }
            }
        }
    }

    // 5a. Assert: The failed commit MUST leave last_committed_tx unchanged
    let last_tx_after_failure = storage.last_tx_id().await.expect("last_tx_id query must succeed");
    assert_eq!(
        last_tx_after_failure, tx1,
        "last_committed_tx MUST remain unchanged (tx1) when commit fails"
    );

    // 5b. Assert: Re-committing the same content after restoring write permissions must succeed
    storage.put(tx2, b"key2", b"val2").await.expect("put tx2 must succeed");
    storage
        .commit(tx2)
        .await
        .expect("Re-committing tx2 after restoring write permissions must succeed");

    let last_tx_final = storage.last_tx_id().await.expect("last_tx_id query must succeed");
    assert_eq!(
        last_tx_final, tx2,
        "last_committed_tx must advance to tx2 after successful re-commit"
    );

    // 5c. Assert: All previously committed entries remain correctly readable without collateral damage
    let val1_check = storage.get(b"key1").await.expect("get key1 must succeed");
    assert_eq!(
        val1_check,
        Some(b"val1".to_vec()),
        "Initial entry key1 must remain intact after recovery"
    );

    let val2_check = storage.get(b"key2").await.expect("get key2 must succeed");
    assert_eq!(
        val2_check,
        Some(b"val2".to_vec()),
        "Re-committed entry key2 must be correctly readable"
    );
}
