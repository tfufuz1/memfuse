use memfuse_checkpoint::PersistentCheckpointStore;
use memfuse_core::{MemFuseError, Result, StorageEngine, StorageStats, TxId};
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

#[async_trait::async_trait]
impl StorageEngine for FaultyMockStorage {
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        Ok(self.data.lock().get(key).cloned())
    }
    async fn put(&self, _tx_id: TxId, key: &[u8], value: &[u8]) -> Result<()> {
        if self.fail_put {
            return Err(MemFuseError::Storage("Disk full / I/O write error".into()));
        }
        self.data.lock().insert(key.to_vec(), value.to_vec());
        Ok(())
    }
    async fn delete(&self, _tx_id: TxId, key: &[u8]) -> Result<()> {
        self.data.lock().remove(key);
        Ok(())
    }
    async fn commit(&self, _tx_id: TxId) -> Result<()> {
        if self.fail_commit {
            return Err(MemFuseError::Storage("Commit failed / fsync error".into()));
        }
        Ok(())
    }
    async fn rollback(&self, _tx_id: TxId) -> Result<()> {
        Ok(())
    }
    async fn rollback_to_tx(&self, _tx_id: TxId) -> Result<()> {
        Ok(())
    }
    async fn flush(&self) -> Result<()> {
        Ok(())
    }
    async fn stats(&self) -> Result<StorageStats> {
        Ok(StorageStats {
            num_segments: 0,
            total_size_bytes: 0,
            memtable_size_bytes: 0,
        })
    }
    async fn pin_checkpoint(&self, seq_no: u64) -> Result<()> {
        self.pinned.lock().insert(seq_no);
        Ok(())
    }
    async fn unpin_checkpoint(&self, seq_no: u64) -> Result<()> {
        self.pinned.lock().remove(&seq_no);
        Ok(())
    }
    async fn get_at_seq(&self, key: &[u8], _seq: u64) -> Result<Option<Vec<u8>>> {
        self.get(key).await
    }
    async fn last_seq_no(&self) -> Result<u64> {
        Ok(0)
    }
    async fn last_tx_id(&self) -> Result<TxId> {
        Ok(TxId::new(0))
    }
    async fn scan(
        &self,
        _start: std::ops::Bound<&[u8]>,
        _end: std::ops::Bound<&[u8]>,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        Ok(Vec::new())
    }
    async fn scan_prefix(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let data = self.data.lock();
        Ok(data
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect())
    }
    async fn scan_prefix_at(&self, prefix: &[u8], _seq_no: u64) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        self.scan_prefix(prefix).await
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
