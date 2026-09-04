#![allow(deprecated)]

use memfuse_checkpoint::{
    clear_all_orphaned_checkpoints, CheckpointManifest, CheckpointMeta, PersistentCheckpointStore,
};
use memfuse_core::{Result, StorageEngine, StorageStats, TxId};
use parking_lot::Mutex;
use proptest::prelude::*;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

struct TrackingMockStorage {
    data: Mutex<HashMap<Vec<u8>, Vec<u8>>>,
    pinned: Mutex<HashSet<u64>>,
    rolled_back_txs: Mutex<Vec<TxId>>,
}

impl TrackingMockStorage {
    fn new() -> Self {
        Self {
            data: Mutex::new(HashMap::new()),
            pinned: Mutex::new(HashSet::new()),
            rolled_back_txs: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait::async_trait]
impl StorageEngine for TrackingMockStorage {
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
    async fn rollback_to_tx(&self, tx_id: TxId) -> Result<()> {
        self.rolled_back_txs.lock().push(tx_id);
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

#[derive(Debug, Clone)]
enum GuardAction {
    Commit,
    Rollback,
    DropWithoutAction,
}

prop_compose! {
    fn arb_guard_step()(
        tx in 1..10_000u64,
        action in prop_oneof![
            Just(GuardAction::Commit),
            Just(GuardAction::Rollback),
            Just(GuardAction::DropWithoutAction)
        ]
    ) -> (u64, GuardAction) {
        (tx, action)
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(20))]

    #[test]
    fn prop_guard_random_lifecycle_sequences(steps in proptest::collection::vec(arb_guard_step(), 1..15)) {
        clear_all_orphaned_checkpoints();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let storage = Arc::new(TrackingMockStorage::new());
            let store = PersistentCheckpointStore::new(storage.clone(), "prop_ns").unwrap();

            let mut expected_rollbacks = Vec::new();

            for (tx_raw, action) in steps {
                let tx = TxId::new(tx_raw);
                let guard = store.create_guard(tx).unwrap();

                match action {
                    GuardAction::Commit => {
                        let res = guard.commit();
                        prop_assert!(res.is_ok());
                    }
                    GuardAction::Rollback => {
                        let res = guard.rollback().await;
                        prop_assert!(res.is_ok());
                        expected_rollbacks.push(tx);
                    }
                    GuardAction::DropWithoutAction => {
                        drop(guard);
                        let recovered = store.recover_orphaned_checkpoints().await;
                        prop_assert!(recovered.is_ok());
                        expected_rollbacks.push(tx);
                    }
                }
            }

            let actual_rollbacks = storage.rolled_back_txs.lock().clone();
            prop_assert_eq!(actual_rollbacks, expected_rollbacks);
            Ok(())
        }).unwrap();
    }

    #[test]
    fn prop_manifest_checksum_integrity(
        name in "[a-zA-Z0-9_]{1,50}",
        collection in "[a-zA-Z0-9_]{1,50}",
        seq in 0..1_000_000u64,
        tx in 0..1_000_000u64,
        components in proptest::collection::vec("[a-zA-Z0-9_]{1,20}", 0..5)
    ) {
        let meta = CheckpointMeta {
            name,
            collection_id: collection,
            seq_no: seq,
            tx_id: TxId::new(tx),
            metadata: serde_json::json!({"prop": true}),
            created_at: 1000,
        };

        let manifest_res = CheckpointManifest::new(meta, components);
        prop_assert!(manifest_res.is_ok());
        let manifest = manifest_res.unwrap();

        prop_assert!(manifest.verify().is_ok());

        let mut tampered = manifest.clone();
        tampered.checksum.push_str("bad");
        prop_assert!(tampered.verify().is_err());
    }
}
