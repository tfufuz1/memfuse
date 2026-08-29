use async_trait::async_trait;
use memfuse_checkpoint::PersistentCheckpointStore;
use memfuse_core::{MemFuseError, Result, StorageEngine, StorageStats, TxId};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;

struct FaultInjectableStorage {
    data: Mutex<HashMap<Vec<u8>, Vec<u8>>>,
    corrupt_on_key: Mutex<Option<Vec<u8>>>,
}

impl FaultInjectableStorage {
    fn new() -> Self {
        Self {
            data: Mutex::new(HashMap::new()),
            corrupt_on_key: Mutex::new(None),
        }
    }
}

#[async_trait]
impl StorageEngine for FaultInjectableStorage {
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        Ok(self.data.lock().get(key).cloned())
    }
    async fn get_at_seq(&self, key: &[u8], _seq: u64) -> Result<Option<Vec<u8>>> {
        self.get(key).await
    }
    async fn put(&self, _tx_id: TxId, key: &[u8], value: &[u8]) -> Result<()> {
        let mut data = self.data.lock();
        if let Some(corrupt_key) = self.corrupt_on_key.lock().as_ref() {
            if key == corrupt_key {
                // Fault injection: insert corrupted payload (partial/invalid JSON)
                data.insert(key.to_vec(), b"{\"invalid\": \"partial_manifest}".to_vec());
                return Ok(());
            }
        }
        data.insert(key.to_vec(), value.to_vec());
        Ok(())
    }
    async fn delete(&self, _tx_id: TxId, key: &[u8]) -> Result<()> {
        self.data.lock().remove(key);
        Ok(())
    }
    async fn commit(&self, _tx_id: TxId) -> Result<()> { Ok(()) }
    async fn rollback(&self, _tx_id: TxId) -> Result<()> { Ok(()) }
    async fn rollback_to_tx(&self, _tx_id: TxId) -> Result<()> { Ok(()) }
    async fn last_seq_no(&self) -> Result<u64> { Ok(0) }
    async fn last_tx_id(&self) -> Result<TxId> { Ok(TxId::new(0)) }
    async fn flush(&self) -> Result<()> { Ok(()) }
    async fn stats(&self) -> Result<StorageStats> {
        Ok(StorageStats {
            num_segments: 0,
            total_size_bytes: 0,
            memtable_size_bytes: 0,
        })
    }
    async fn pin_checkpoint(&self, _seq_no: u64) -> Result<()> { Ok(()) }
    async fn unpin_checkpoint(&self, _seq_no: u64) -> Result<()> { Ok(()) }
    async fn scan_prefix(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let data = self.data.lock();
        Ok(data
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect())
    }
    async fn scan(
        &self,
        _s: std::ops::Bound<&[u8]>,
        _e: std::ops::Bound<&[u8]>,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        Ok(Vec::new())
    }
}

#[tokio::test]
async fn test_restore_rejects_corrupted_or_partial_manifest() {
    let storage = Arc::new(FaultInjectableStorage::new());
    let store = PersistentCheckpointStore::new(storage.clone(), "test_ns");

    let cp_key = b"test_ns:checkpoint:faulty_cp";
    *storage.corrupt_on_key.lock() = Some(cp_key.to_vec());

    // Create checkpoint with fault injection enabled for storing manifest
    let res = store
        .create_checkpoint("faulty_cp", "c1", 100, TxId::new(5), serde_json::json!({}))
        .await;

    assert!(res.is_ok());

    // Reopen store on existing storage to force read from storage instead of cache
    let reloaded_store = PersistentCheckpointStore::new(storage.clone(), "test_ns");
    let get_res = reloaded_store.get_checkpoint("faulty_cp").await;

    assert!(
        get_res.is_err(),
        "Restoring or loading checkpoint with corrupted/partial manifest MUST fail"
    );

    if let Err(err) = get_res {
        assert!(matches!(err, MemFuseError::Serialization(_)));
    }
}

#[tokio::test]
async fn test_restore_rejects_tampered_manifest_checksum() {
    let storage = Arc::new(FaultInjectableStorage::new());
    let store = PersistentCheckpointStore::new(storage.clone(), "test_ns");

    store
        .create_checkpoint("tampered_cp", "c1", 101, TxId::new(6), serde_json::json!({}))
        .await
        .unwrap();

    // Directly tamper with stored manifest payload (alter sequence number inside json string while keeping JSON valid)
    let cp_key = b"test_ns:checkpoint:tampered_cp";
    let stored_bytes = storage.data.lock().get(cp_key.as_slice()).unwrap().clone();
    let mut json_val: serde_json::Value = serde_json::from_slice(&stored_bytes).unwrap();

    json_val["meta"]["seq_no"] = serde_json::json!(9999);
    let tampered_bytes = serde_json::to_vec(&json_val).unwrap();
    storage.data.lock().insert(cp_key.to_vec(), tampered_bytes);

    // Reopen store to clear in-memory cache
    let reloaded_store = PersistentCheckpointStore::new(storage.clone(), "test_ns");
    let restore_res = reloaded_store.restore_checkpoint("tampered_cp").await;

    assert!(
        restore_res.is_err(),
        "Tampered manifest payload MUST be rejected by checksum validation"
    );

    if let Err(MemFuseError::Serialization(msg)) = restore_res {
        assert!(msg.contains("checksum mismatch"));
    } else {
        panic!("Expected Serialization error with checksum mismatch");
    }
}
