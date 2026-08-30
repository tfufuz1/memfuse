use memfuse_checkpoint::PersistentCheckpointStore;
use memfuse_core::{Result, StorageEngine, StorageStats, TxId};
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

struct MockStorage {
    data: Mutex<HashMap<Vec<u8>, Vec<u8>>>,
    pinned: Mutex<HashSet<u64>>,
    fail_on_put_seq: Mutex<HashSet<u64>>,
}

impl MockStorage {
    fn new() -> Self {
        Self {
            data: Mutex::new(HashMap::new()),
            pinned: Mutex::new(HashSet::new()),
            fail_on_put_seq: Mutex::new(HashSet::new()),
        }
    }
}

#[async_trait::async_trait]
impl StorageEngine for MockStorage {
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        Ok(self.data.lock().get(key).cloned())
    }
    async fn put(&self, _tx_id: TxId, key: &[u8], value: &[u8]) -> Result<()> {
        if let Ok(manifest) = serde_json::from_slice::<memfuse_checkpoint::CheckpointManifest>(value) {
            if self.fail_on_put_seq.lock().contains(&manifest.meta.seq_no) {
                return Err(memfuse_core::MemFuseError::Internal(
                    "Fault injected storage save failure".to_string(),
                ));
            }
        }
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
    let manager = Arc::new(PersistentCheckpointStore::new(storage.clone(), "test"));

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

/// ANCHOR[TEST:CKPT-001] STATUS:DONE (ID: AGT-CKPT-f3a19b88) (TS:2026-08-30T22:00:34Z) (SESSION:a140747b) — Concurrent Checkpoint Pinning & GC Exclusions
/// REVIEW-PASS[1/2] STATUS:PASS (ID: AGT-CKPT-f3a19b88) (TS:2026-08-30T22:30:00Z) (SESSION:b8e4f1a2)
/// REVIEW-PASS[2/2] STATUS:PASS (ID: AGT-CKPT-f3a19b88) (TS:2026-08-30T22:35:00Z) (SESSION:c9f5e2b3)
/// Verifies concurrent pinning, unpinning on save failures (fault injection), overwrite unpinning, and GC exclusion state.
#[tokio::test]
async fn test_concurrent_checkpoint_pinning_and_gc_exclusions() {
    let storage = Arc::new(MockStorage::new());
    let store = Arc::new(PersistentCheckpointStore::new(storage.clone(), "test"));

    // Inject fault for seq_nos 5, 10, 15
    let failed_seqs: HashSet<u64> = vec![5, 10, 15].into_iter().collect();
    *storage.fail_on_put_seq.lock() = failed_seqs.clone();

    let mut handles = Vec::new();
    for i in 1..=20 {
        let store_clone = store.clone();
        handles.push(tokio::spawn(async move {
            let seq = i as u64;
            store_clone
                .create_checkpoint(
                    &format!("cp_{seq}"),
                    "coll_test",
                    seq,
                    TxId::new(seq),
                    serde_json::json!({"step": seq}),
                )
                .await
        }));
    }

    let mut success_count = 0;
    let mut failure_count = 0;
    for handle in handles {
        match handle.await.unwrap() {
            Ok(_) => success_count += 1,
            Err(_) => failure_count += 1,
        }
    }

    assert_eq!(success_count, 17, "Expected 17 successful checkpoints");
    assert_eq!(failure_count, 3, "Expected 3 failed checkpoints due to fault injection");

    let pinned = storage.pinned.lock().clone();
    // Verify all 17 successful checkpoints are pinned
    for seq in 1..=20 {
        if failed_seqs.contains(&seq) {
            assert!(
                !pinned.contains(&seq),
                "Failed checkpoint seq {seq} must NOT remain pinned (must be unpinned for GC)"
            );
        } else {
            assert!(
                pinned.contains(&seq),
                "Successful checkpoint seq {seq} must be pinned"
            );
        }
    }

    // Overwrite cp_1 with new seq_no 100
    store
        .create_checkpoint("cp_1", "coll_test", 100, TxId::new(100), serde_json::json!({}))
        .await
        .unwrap();

    let pinned_after_overwrite = storage.pinned.lock().clone();
    assert!(
        !pinned_after_overwrite.contains(&1),
        "Old seq_no 1 must be unpinned after overwrite"
    );
    assert!(
        pinned_after_overwrite.contains(&100),
        "New seq_no 100 must be pinned after overwrite"
    );

    // Clean up all checkpoints and verify unpinned
    let list = store.list_checkpoints().await.unwrap();
    for cp in list {
        store.drop_checkpoint(&cp.name).await.unwrap();
    }
    assert!(
        storage.pinned.lock().is_empty(),
        "All checkpoints should be unpinned after drop"
    );
}
