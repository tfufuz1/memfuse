use memfuse_checkpoint::PersistentCheckpointStore;
use memfuse_core::{BoxFuture, Result, StorageEngine, StorageStats, TxId};
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

impl StorageEngine for MockStorage {
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
        Box::pin(async move { Ok(()) })
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

#[tokio::test]
async fn test_concurrent_checkpoint_creation_same_name() {
    let storage = Arc::new(MockStorage::new());
    let manager = Arc::new(PersistentCheckpointStore::new(storage.clone(), "test").unwrap());

    let mut handles = Vec::new();
    for i in 0..10 {
        let m = manager.clone();
        handles.push(tokio::spawn(async move {
            // Correct API call matching create_checkpoint(name, collection_id, seq_no, tx_id, metadata)
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

#[tokio::test]
async fn test_concurrent_drop_checkpoints() {
    let storage = Arc::new(MockStorage::new());
    let manager = Arc::new(PersistentCheckpointStore::new(storage.clone(), "test").unwrap());

    // Create 10 checkpoints
    for i in 0..10 {
        manager
            .create_checkpoint(
                &format!("cp_{i}"),
                "coll",
                i as u64,
                TxId::new(i as u64),
                serde_json::json!({}),
            )
            .await
            .unwrap();
    }

    let mut handles = Vec::new();
    for i in 0..10 {
        let m = manager.clone();
        handles.push(tokio::spawn(async move {
            m.drop_checkpoint(&format!("cp_{i}")).await
        }));
    }

    for h in handles {
        h.await.unwrap().unwrap();
    }

    let remaining = manager.list_checkpoints().await.unwrap();
    assert!(
        remaining.is_empty(),
        "All checkpoints should have been dropped concurrently"
    );
    assert!(
        storage.pinned.lock().is_empty(),
        "All checkpoint seq_nos should be unpinned after drops"
    );
}
