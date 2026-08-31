use memfuse_checkpoint::PersistentCheckpointStore;
use memfuse_core::{Result, StorageEngine, StorageStats, TxId};
use parking_lot::Mutex;
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

/// A TxId-versioned storage engine supporting exact time-travel rollbacks.
struct VersionedMockStorage {
    /// key -> (tx_id -> value)
    store: Mutex<BTreeMap<Vec<u8>, BTreeMap<u64, Option<Vec<u8>>>>>,
    pinned: Mutex<HashSet<u64>>,
}

impl VersionedMockStorage {
    fn new() -> Self {
        Self {
            store: Mutex::new(BTreeMap::new()),
            pinned: Mutex::new(HashSet::new()),
        }
    }

    /// Computes deterministic BLAKE3 hash over all active key-value pairs at current state.
    fn state_checksum(&self) -> String {
        let store = self.store.lock();
        let mut hasher = blake3::Hasher::new();
        for (key, versions) in store.iter() {
            if let Some((_, Some(val))) = versions.iter().next_back() {
                hasher.update(key);
                hasher.update(val);
            }
        }
        hasher.finalize().to_hex().to_string()
    }
}

#[async_trait::async_trait]
impl StorageEngine for VersionedMockStorage {
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let store = self.store.lock();
        if let Some(versions) = store.get(key) {
            if let Some((_, val_opt)) = versions.iter().next_back() {
                return Ok(val_opt.clone());
            }
        }
        Ok(None)
    }

    async fn put(&self, tx_id: TxId, key: &[u8], value: &[u8]) -> Result<()> {
        let mut store = self.store.lock();
        store
            .entry(key.to_vec())
            .or_default()
            .insert(tx_id.0, Some(value.to_vec()));
        Ok(())
    }

    async fn delete(&self, tx_id: TxId, key: &[u8]) -> Result<()> {
        let mut store = self.store.lock();
        store
            .entry(key.to_vec())
            .or_default()
            .insert(tx_id.0, None);
        Ok(())
    }

    async fn commit(&self, _tx_id: TxId) -> Result<()> {
        Ok(())
    }
    async fn rollback(&self, _tx_id: TxId) -> Result<()> {
        Ok(())
    }

    async fn rollback_to_tx(&self, tx_id: TxId) -> Result<()> {
        let target_tx = tx_id.0;
        let mut store = self.store.lock();
        for versions in store.values_mut() {
            versions.retain(|&v_tx, _| v_tx <= target_tx);
        }
        // Remove keys that now have no version entries
        store.retain(|_, versions| !versions.is_empty());
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
        let store = self.store.lock();
        let mut res = Vec::new();
        for (k, versions) in store.iter() {
            if k.starts_with(prefix) {
                if let Some((_, Some(v))) = versions.iter().next_back() {
                    res.push((k.clone(), v.clone()));
                }
            }
        }
        Ok(res)
    }

    async fn scan_prefix_at(&self, prefix: &[u8], _seq_no: u64) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        self.scan_prefix(prefix).await
    }
}

/// Time-Travel Test: State A -> CP1 -> State B -> CP2 -> State C -> Rollback to CP1.
/// Verifies byte-exact checksum equality between initial State A and restored State A.
#[tokio::test]
async fn test_time_travel_sequence_byte_exact_recovery() {
    let storage = Arc::new(VersionedMockStorage::new());
    let store = PersistentCheckpointStore::new(storage.clone(), "ns_tt");

    // 1. Establish State A
    let tx_a = TxId::new(10);
    storage.put(tx_a, b"doc_1", b"content_A1").await.unwrap();
    storage.put(tx_a, b"doc_2", b"content_A2").await.unwrap();

    let checksum_a = storage.state_checksum();

    // Create Checkpoint 1 at State A
    let _cp1 = store
        .create_checkpoint("cp1", "col_tt", 1, tx_a, serde_json::json!({"state": "A"}))
        .await
        .unwrap();

    // 2. Transition to State B
    let tx_b = TxId::new(20);
    storage.put(tx_b, b"doc_1", b"content_B1_updated").await.unwrap();
    storage.put(tx_b, b"doc_3", b"content_B3_new").await.unwrap();

    let checksum_b = storage.state_checksum();
    assert_ne!(checksum_a, checksum_b, "State B checksum must differ from State A");

    // Create Checkpoint 2 at State B
    let _cp2 = store
        .create_checkpoint("cp2", "col_tt", 2, tx_b, serde_json::json!({"state": "B"}))
        .await
        .unwrap();

    // 3. Transition to State C
    let tx_c = TxId::new(30);
    storage.delete(tx_c, b"doc_2").await.unwrap();
    storage.put(tx_c, b"doc_4", b"content_C4").await.unwrap();

    let checksum_c = storage.state_checksum();
    assert_ne!(checksum_b, checksum_c, "State C checksum must differ from State B");

    // 4. Time-Travel: Restore Checkpoint 1 (State A)
    let restored_meta = store.restore_checkpoint("cp1").await.unwrap();
    assert_eq!(restored_meta.name, "cp1");
    assert_eq!(restored_meta.tx_id, tx_a);

    // 5. Verify byte-exact checksum match with original State A!
    let checksum_restored = storage.state_checksum();
    assert_eq!(
        checksum_a, checksum_restored,
        "Restored state checksum must match State A byte-for-byte!"
    );

    // Verify individual key contents
    assert_eq!(storage.get(b"doc_1").await.unwrap(), Some(b"content_A1".to_vec()));
    assert_eq!(storage.get(b"doc_2").await.unwrap(), Some(b"content_A2".to_vec()));
    assert_eq!(storage.get(b"doc_3").await.unwrap(), None);
    assert_eq!(storage.get(b"doc_4").await.unwrap(), None);
}
