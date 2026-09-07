use memfuse_checkpoint::PersistentCheckpointStore;
use memfuse_core::{BoxFuture, MemFuseError, Result, StorageEngine, StorageStats, TxId};
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

struct FaultyMockStorage {
    data: Mutex<HashMap<Vec<u8>, Vec<u8>>>,
    pinned: Mutex<HashSet<u64>>,
    fail_put: bool,
    fail_commit: bool,
}

impl FaultyMockStorage {
    fn new(fail_put: bool, fail_commit: bool) -> Self {
        Self {
            data: Mutex::new(HashMap::new()),
            pinned: Mutex::new(HashSet::new()),
            fail_put,
            fail_commit,
        }
    }
}

impl StorageEngine for FaultyMockStorage {
    fn get<'a>(&'a self, key: &'a [u8]) -> BoxFuture<'a, Result<Option<Vec<u8>>>> {
        Box::pin(async move { Ok(self.data.lock().get(key).cloned()) })
    }
    fn put<'a>(
        &'a self,
        _tx_id: TxId,
        key: &'a [u8],
        value: &'a [u8],
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            if self.fail_put {
                return Err(MemFuseError::Storage("Disk full / I/O write error".into()));
            }
            self.data.lock().insert(key.to_vec(), value.to_vec());
            Ok(())
        })
    }
    fn delete<'a>(&'a self, _tx_id: TxId, key: &'a [u8]) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.data.lock().remove(key);
            Ok(())
        })
    }
    fn commit<'a>(&'a self, _tx_id: TxId) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            if self.fail_commit {
                return Err(MemFuseError::Storage("Commit failed / fsync error".into()));
            }
            Ok(())
        })
    }
    fn rollback<'a>(&'a self, _tx_id: TxId) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move { Ok(()) })
    }
    fn rollback_to_tx<'a>(&'a self, _tx_id: TxId) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move { Ok(()) })
    }
    fn flush<'a>(&'a self) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move { Ok(()) })
    }
    fn stats<'a>(&'a self) -> BoxFuture<'a, Result<StorageStats>> {
        Box::pin(async move {
            Ok(StorageStats {
                num_segments: 0,
                total_size_bytes: 0,
                memtable_size_bytes: 0,
            })
        })
    }
    fn pin_checkpoint<'a>(&'a self, seq_no: u64) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.pinned.lock().insert(seq_no);
            Ok(())
        })
    }
    fn unpin_checkpoint<'a>(&'a self, seq_no: u64) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.pinned.lock().remove(&seq_no);
            Ok(())
        })
    }
    fn get_at_seq<'a>(
        &'a self,
        key: &'a [u8],
        _seq: u64,
    ) -> BoxFuture<'a, Result<Option<Vec<u8>>>> {
        Box::pin(async move { self.get(key).await })
    }
    fn last_seq_no<'a>(&'a self) -> BoxFuture<'a, Result<u64>> {
        Box::pin(async move { Ok(0) })
    }
    fn last_tx_id<'a>(&'a self) -> BoxFuture<'a, Result<TxId>> {
        Box::pin(async move { Ok(TxId::new(0)) })
    }
    fn scan<'a>(
        &'a self,
        _start: std::ops::Bound<&'a [u8]>,
        _end: std::ops::Bound<&'a [u8]>,
    ) -> BoxFuture<'a, Result<Vec<(Vec<u8>, Vec<u8>)>>> {
        Box::pin(async move { Ok(Vec::new()) })
    }
    fn scan_prefix<'a>(
        &'a self,
        prefix: &'a [u8],
    ) -> BoxFuture<'a, Result<Vec<(Vec<u8>, Vec<u8>)>>> {
        Box::pin(async move {
            let data = self.data.lock();
            Ok(data
                .iter()
                .filter(|(k, _)| k.starts_with(prefix))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect())
        })
    }
    fn scan_prefix_at<'a>(
        &'a self,
        prefix: &'a [u8],
        _seq_no: u64,
    ) -> BoxFuture<'a, Result<Vec<(Vec<u8>, Vec<u8>)>>> {
        Box::pin(async move { self.scan_prefix(prefix).await })
    }
}

/// Test Storage Write Failure: returns `Err(MemFuseError::Storage)`, no panic, seq_no unpinned.
#[tokio::test]
async fn test_storage_put_failure_handling() {
    let storage = Arc::new(FaultyMockStorage::new(true, false));
    let store = PersistentCheckpointStore::new(storage.clone(), "ns_err").unwrap();

    let res = store
        .create_checkpoint("fail_cp", "col1", 99, TxId::new(1), serde_json::json!({}))
        .await;

    assert!(
        res.is_err(),
        "Storage write failure must return Result::Err"
    );
    if let Err(e) = res {
        assert!(
            matches!(e, MemFuseError::Storage(_)),
            "Error variant must be MemFuseError::Storage"
        );
    }
    assert!(
        !storage.pinned.lock().contains(&99),
        "Failed sequence number must be unpinned upon storage write error"
    );
}

/// Test Storage Commit Failure: returns `Err(MemFuseError::Storage)`, no panic, seq_no unpinned.
#[tokio::test]
async fn test_storage_commit_failure_handling() {
    let storage = Arc::new(FaultyMockStorage::new(false, true));
    let store = PersistentCheckpointStore::new(storage.clone(), "ns_err").unwrap();

    let res = store
        .create_checkpoint(
            "commit_fail_cp",
            "col1",
            100,
            TxId::new(1),
            serde_json::json!({}),
        )
        .await;

    assert!(
        res.is_err(),
        "Storage commit failure must return Result::Err"
    );
    assert!(
        !storage.pinned.lock().contains(&100),
        "Failed sequence number must be unpinned upon storage commit error"
    );
}

/// Test Restore Nonexistent Checkpoint Name: returns `MemFuseError::CheckpointNotFound`.
#[tokio::test]
async fn test_restore_nonexistent_checkpoint_returns_not_found() {
    let storage = Arc::new(FaultyMockStorage::new(false, false));
    let store = PersistentCheckpointStore::new(storage, "ns_err").unwrap();

    let res = store.restore_checkpoint("does_not_exist").await;
    assert!(
        matches!(res, Err(MemFuseError::CheckpointNotFound)),
        "Restoring nonexistent checkpoint must return MemFuseError::CheckpointNotFound"
    );
}

/// Test Boundary Validation: empty name, whitespace name, empty collection_id, oversized name (>256 chars).
#[tokio::test]
async fn test_identifier_input_boundary_validation() {
    let storage = Arc::new(FaultyMockStorage::new(false, false));
    let store = PersistentCheckpointStore::new(storage, "ns_err").unwrap();

    // 1. Empty checkpoint name
    let res = store
        .create_checkpoint("", "col1", 1, TxId::new(1), serde_json::json!({}))
        .await;
    assert!(matches!(res, Err(MemFuseError::InvalidInput(_))));

    // 2. Whitespace-only checkpoint name
    let res = store
        .create_checkpoint("   \t  ", "col1", 1, TxId::new(1), serde_json::json!({}))
        .await;
    assert!(matches!(res, Err(MemFuseError::InvalidInput(_))));

    // 3. Empty collection ID
    let res = store
        .create_checkpoint("cp1", "", 1, TxId::new(1), serde_json::json!({}))
        .await;
    assert!(matches!(res, Err(MemFuseError::InvalidInput(_))));

    // 4. Oversized checkpoint name (> 256 chars)
    let long_name = "x".repeat(257);
    let res = store
        .create_checkpoint(&long_name, "col1", 1, TxId::new(1), serde_json::json!({}))
        .await;
    assert!(matches!(res, Err(MemFuseError::InvalidInput(_))));

    // 5. Drop checkpoint with empty name
    let res = store.drop_checkpoint("").await;
    assert!(matches!(res, Err(MemFuseError::InvalidInput(_))));

    // 6. Get checkpoint with empty name
    let res = store.get_checkpoint("   ").await;
    assert!(matches!(res, Err(MemFuseError::InvalidInput(_))));
}
