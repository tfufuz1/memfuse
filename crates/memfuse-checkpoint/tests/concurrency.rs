use memfuse_checkpoint::PersistentCheckpointStore;
use memfuse_core::{Result, StorageEngine, StorageStats, TxId};
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

struct MockStorage {
    data: Mutex<HashMap<Vec<u8>, Vec<u8>>>,
    pinned: Mutex<HashSet<u64>>,
}

impl MockStorage {
    fn new() -> Self {
        Self {
            data: Mutex::new(HashMap::new()),
            pinned: Mutex::new(HashSet::new()),
        }
    }
}

#[async_trait::async_trait]
impl StorageEngine for MockStorage {
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        Ok(self.data.lock().get(key).cloned())
    }
    async fn put(&self, _tx_id: TxId, key: &[u8], value: &[u8]) -> Result<()> {
        self.data.lock().insert(key.to_vec(), value.to_vec());
        Ok(())
    }
    async fn delete(&self, _tx_id: TxId, key: &[u8]) -> Result<()> {
        self.data.lock().remove(key);
        Ok(())
    }
    async fn commit(&self, _tx_id: TxId) -> Result<()> {
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

#[tokio::test]
async fn test_concurrent_checkpoint_creation_same_name() {
    let storage = Arc::new(MockStorage::new());
    let manager = Arc::new(PersistentCheckpointStore::new(storage.clone(), "test"));

    let mut handles = Vec::new();
    for i in 0..10 {
        let m = manager.clone();
        handles.push(tokio::spawn(async move {
            m.create_checkpoint(
                "same_name",
                "coll",
                i as u64,
                TxId::new(i as u64),
                serde_json::json!({}),
            )
            .await
        }));
    }

    for h in handles {
        h.await.unwrap().unwrap();
    }

    let checkpoints = manager.list_checkpoints().await.unwrap();
    // In a world without race conditions and duplicate names, this should probably be 1 if we expect overwrite,
    // or it should have failed. Currently it probably has 1.
    println!("Number of checkpoints: {}", checkpoints.len());

    assert!(
        checkpoints.len() <= 1,
        "Should not have multiple checkpoints with the same name, found {}",
        checkpoints.len()
    );
}
