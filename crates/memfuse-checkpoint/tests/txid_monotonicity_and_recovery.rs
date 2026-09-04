use memfuse_checkpoint::{CheckpointManifest, CheckpointMeta, PersistentCheckpointStore};
use memfuse_core::{MemFuseError, Result, StorageEngine, StorageStats, TxId};
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

struct MockPersistentStorage {
    data: Mutex<HashMap<Vec<u8>, Vec<u8>>>,
    pinned: Mutex<HashSet<u64>>,
}

impl MockPersistentStorage {
    fn new() -> Self {
        Self {
            data: Mutex::new(HashMap::new()),
            pinned: Mutex::new(HashSet::new()),
        }
    }
}

#[async_trait::async_trait]
impl StorageEngine for MockPersistentStorage {
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        Ok(self.data.lock().get(key).cloned())
    }
    async fn get_at_seq(&self, key: &[u8], _seq: u64) -> Result<Option<Vec<u8>>> {
        self.get(key).await
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
    async fn last_seq_no(&self) -> Result<u64> {
        Ok(0)
    }
    async fn last_tx_id(&self) -> Result<TxId> {
        Ok(TxId::new(0))
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

/// Test 1: Monotonicity across simulated process restarts.
/// Store is populated with several `allocate_tx()` calls and checkpoints.
/// A new store instance is created over the same storage.
/// Verifies that the next allocated TxId is strictly greater than all previously allocated TxIds.
#[tokio::test]
async fn test_txid_monotonicity_across_process_restart() {
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("MEMFUSE_ORPHAN_PIN_PATH", tmp.path());
    let storage = Arc::new(MockPersistentStorage::new());

    let mut allocated_txs = Vec::new();

    // First process run
    {
        let store1 = PersistentCheckpointStore::open(storage.clone(), "ns_mono")
            .await
            .unwrap();

        for i in 1..=5 {
            let _cp = store1
                .create_checkpoint(
                    &format!("cp_{i}"),
                    "col_1",
                    i,
                    TxId::new(100 + i),
                    serde_json::json!({"step": i}),
                )
                .await
                .unwrap();
            allocated_txs.push(store1.allocate_tx().await.unwrap());
        }
    }

    let max_previous_tx = allocated_txs.iter().map(|tx| tx.inner()).max().unwrap();

    // Simulated process restart: open new PersistentCheckpointStore instance
    let store2 = PersistentCheckpointStore::open(storage.clone(), "ns_mono")
        .await
        .unwrap();

    let post_restart_tx = store2.allocate_tx().await.unwrap();

    assert!(
        post_restart_tx.inner() > max_previous_tx,
        "TxId after restart ({}) must be strictly greater than max previous TxId ({})",
        post_restart_tx.inner(),
        max_previous_tx
    );

    // Verify persisted counter in storage guarantees monotonicity over previous run
    let counter_key = b"ns_mono:checkpoint:__sys_tx_counter__";
    assert!(storage.data.lock().contains_key(counter_key.as_slice()));
}

/// Test 2: Crash-recovery test.
/// Counter metadata entry is missing or corrupted, but store contains existing checkpoints.
/// Verifies that the highest existing TxId is reconstructed and next allocated TxId is strictly greater.
#[tokio::test]
async fn test_crash_recovery_reconstructs_highest_txid_when_meta_missing_or_corrupt() {
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("MEMFUSE_ORPHAN_PIN_PATH", tmp.path());
    let storage = Arc::new(MockPersistentStorage::new());

    // 1. Manually populate storage with existing checkpoints (simulating pre-crash store)
    let highest_raw = 42u64;
    let highest_tx = TxId::new(TxId::INTERNAL_BASE + highest_raw);

    let meta = CheckpointMeta {
        name: "pre_crash_cp".to_string(),
        collection_id: "col_recovery".to_string(),
        seq_no: 1,
        tx_id: highest_tx,
        metadata: serde_json::json!({}),
        created_at: 1000,
    };
    let manifest = CheckpointManifest::new(meta, vec!["storage".to_string()]).unwrap();
    let value = serde_json::to_vec(&manifest).unwrap();

    let cp_key = b"ns_rec:checkpoint:pre_crash_cp";
    storage.data.lock().insert(cp_key.to_vec(), value);

    // Ensure the counter metadata key is NOT present (simulating crash before/during meta write)
    let meta_key = b"ns_rec:checkpoint:__sys_tx_counter__";
    assert!(!storage.data.lock().contains_key(meta_key.as_slice()));

    // 2. Open new PersistentCheckpointStore instance
    let store = PersistentCheckpointStore::open(storage.clone(), "ns_rec")
        .await
        .unwrap();

    // Allocate next internal TxId from store and create a checkpoint with it
    let next_allocated = store.allocate_tx().await.unwrap();
    store
        .create_checkpoint(
            "post_recovery_cp",
            "col_recovery",
            2,
            next_allocated,
            serde_json::json!({}),
        )
        .await
        .unwrap();

    assert!(
        next_allocated.inner() > highest_tx.inner(),
        "Recovered TxId ({}) must be strictly greater than highest pre-crash TxId ({})",
        next_allocated.inner(),
        highest_tx.inner()
    );

    // Also test corrupt counter metadata
    storage
        .data
        .lock()
        .insert(meta_key.to_vec(), b"corrupted json bytes {{{".to_vec());

    let store_corrupt = PersistentCheckpointStore::open(storage.clone(), "ns_rec")
        .await
        .unwrap();

    let corrupt_recovered_tx = store_corrupt.allocate_tx().await.unwrap();

    assert!(
        corrupt_recovered_tx.inner() > next_allocated.inner(),
        "TxId after corrupt meta recovery ({}) must be strictly greater than previous TxId ({})",
        corrupt_recovered_tx.inner(),
        next_allocated.inner()
    );
}

/// Test 3: Consistency check failure on TxId collision / regression.
/// Persisted counter metadata entry has high_water_mark < highest TxId found in store checkpoints.
/// Verifies that PersistentCheckpointStore::open returns a hard error MemFuseError::Internal.
#[tokio::test]
async fn test_txid_regression_collision_check_returns_hard_error() {
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("MEMFUSE_ORPHAN_PIN_PATH", tmp.path());
    let storage = Arc::new(MockPersistentStorage::new());

    // Insert a checkpoint with raw counter = 50
    let scanned_raw = 50u64;
    let scanned_tx = TxId::new(TxId::INTERNAL_BASE + scanned_raw);

    let meta = CheckpointMeta {
        name: "existing_cp".to_string(),
        collection_id: "col_check".to_string(),
        seq_no: 1,
        tx_id: scanned_tx,
        metadata: serde_json::json!({}),
        created_at: 1000,
    };
    let manifest = CheckpointManifest::new(meta, vec!["storage".to_string()]).unwrap();
    let value = serde_json::to_vec(&manifest).unwrap();
    storage
        .data
        .lock()
        .insert(b"ns_check:checkpoint:existing_cp".to_vec(), value);

    // Insert corrupted/regressed counter metadata with high_water_mark = 10 (< 50)
    let regressed_meta = serde_json::json!({ "high_water_mark": 10u64 });
    let meta_bytes = serde_json::to_vec(&regressed_meta).unwrap();
    storage.data.lock().insert(
        b"ns_check:checkpoint:__sys_tx_counter__".to_vec(),
        meta_bytes,
    );

    // Attempting to open store must fail with hard error
    let res = PersistentCheckpointStore::open(storage, "ns_check").await;

    assert!(
        res.is_err(),
        "Store open must fail when persisted HWM < scanned highest TxId"
    );
    if let Err(MemFuseError::Internal(msg)) = res {
        assert!(
            msg.contains("TxId collision / regression detected"),
            "Error message must indicate TxId collision / regression: {msg}"
        );
    } else {
        panic!("Expected MemFuseError::Internal error variant");
    }
}
