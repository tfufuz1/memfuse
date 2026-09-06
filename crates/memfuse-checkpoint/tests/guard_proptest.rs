#![allow(deprecated)]

use memfuse_checkpoint::{
    clear_all_orphaned_checkpoints, CheckpointManifest, CheckpointMeta, PersistentCheckpointStore,
};
use memfuse_core::{
    BoxFuture, Result, StorageEngine, StorageStats, TxId};
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

impl StorageEngine for TrackingMockStorage {
    fn get<'a>(&'a self, key: &'a [u8]) -> BoxFuture<'a, Result<Option<Vec<u8>>>> {
        Box::pin(async move {
        Ok(self.data.lock().get(key).cloned())

        })
    }
    fn put<'a>(&'a self, _tx_id: TxId, key: &'a [u8], value: &'a [u8]) -> BoxFuture<'a, Result<()>> {
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
        Box::pin(async move {
        Ok(())

        })
    }
    fn rollback<'a>(&'a self, _tx_id: TxId) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
        Ok(())

        })
    }
    fn rollback_to_tx<'a>(&'a self, tx_id: TxId) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
        self.rolled_back_txs.lock().push(tx_id);
        Ok(())

        })
    }
    fn flush<'a>(&'a self) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
        Ok(())

        })
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
    fn get_at_seq<'a>(&'a self, key: &'a [u8], _seq: u64) -> BoxFuture<'a, Result<Option<Vec<u8>>>> {
        Box::pin(async move {
        self.get(key).await

        })
    }
    fn last_seq_no<'a>(&'a self) -> BoxFuture<'a, Result<u64>> {
        Box::pin(async move {
        Ok(0)

        })
    }
    fn last_tx_id<'a>(&'a self) -> BoxFuture<'a, Result<TxId>> {
        Box::pin(async move {
        Ok(TxId::new(0))

        })
    }
    fn scan<'a>(
        &'a self,
        _start: std::ops::Bound<&'a [u8]>,
        _end: std::ops::Bound<&'a [u8]>,
    ) -> BoxFuture<'a, Result<Vec<(Vec<u8>, Vec<u8>)>>> {
        Box::pin(async move {
        Ok(Vec::new())

        })
    }
    fn scan_prefix<'a>(&'a self, prefix: &'a [u8]) -> BoxFuture<'a, Result<Vec<(Vec<u8>, Vec<u8>)>>> {
        Box::pin(async move {
        let data = self.data.lock();
        Ok(data
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect())

        })
    }
    fn scan_prefix_at<'a>(&'a self, prefix: &'a [u8], _seq_no: u64) -> BoxFuture<'a, Result<Vec<(Vec<u8>, Vec<u8>)>>> {
        Box::pin(async move {
        self.scan_prefix(prefix).await

        })
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
