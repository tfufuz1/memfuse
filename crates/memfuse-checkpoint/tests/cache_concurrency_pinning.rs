// ANCHOR[TEST:CKPT-001] STATUS:DONE (TS:2026-09-02T23:18:12Z) (SESSION: 358e3b0a)
// AUFGABE : Verify Concurrent Checkpoint Pinning & GC Exclusions
// GATE    : cargo test -p memfuse-checkpoint --test cache_concurrency_pinning
// REVIEW-PASS[1/2] STATUS:PASS (ID: TEST:CKPT-001) (TS:2026-09-01T23:09:00Z) (SESSION: fdf7a62e) PRÜFER-KONTEXT: FRESH - Test covers concurrent pinning, GC safety, and cache behavior.
// REVIEW-PASS[2/2] STATUS:PASS (ID: TEST:CKPT-001) (TS:2026-09-02T23:18:12Z) (SESSION: 2155aaa2) PRÜFER-KONTEXT: FRESH - Confirmed concurrent stress test, cache hit/miss reloading, and unpinning lifecycle.

use memfuse_checkpoint::PersistentCheckpointStore;
use memfuse_core::{
    BoxFuture, Result, StorageEngine, StorageStats, TxId};
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::task::JoinSet;

struct TrackingMockStorage {
    data: Mutex<HashMap<Vec<u8>, Vec<u8>>>,
    pinned: Mutex<HashSet<u64>>,
    get_count: Mutex<usize>,
}

impl TrackingMockStorage {
    fn new() -> Self {
        Self {
            data: Mutex::new(HashMap::new()),
            pinned: Mutex::new(HashSet::new()),
            get_count: Mutex::new(0),
        }
    }
}

impl StorageEngine for TrackingMockStorage {
    fn get<'a>(&'a self, key: &'a [u8]) -> BoxFuture<'a, Result<Option<Vec<u8>>>> {
        Box::pin(async move {
        *self.get_count.lock() += 1;
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
    fn rollback_to_tx<'a>(&'a self, _tx_id: TxId) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
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

/// Test Cache Hit vs Cache Miss reloading from storage.
#[tokio::test]
async fn test_cache_hit_and_miss_reloading() {
    let storage = Arc::new(TrackingMockStorage::new());
    let store1 = PersistentCheckpointStore::new(storage.clone(), "ns_cache").unwrap();

    // Create checkpoint via store1 (populates cache)
    store1
        .create_checkpoint(
            "cp_cache",
            "col1",
            100,
            TxId::new(1),
            serde_json::json!({"v": 1}),
        )
        .await
        .unwrap();

    let initial_get_count = *storage.get_count.lock();

    // First read on store1 -> Cache Hit! Storage `get()` should NOT be called
    let cp_hit = store1.get_checkpoint("cp_cache").await.unwrap().unwrap();
    assert_eq!(cp_hit.seq_no, 100);
    assert_eq!(
        *storage.get_count.lock(),
        initial_get_count,
        "Cache hit must not touch storage"
    );

    // Fresh store instance over same storage -> Cache Miss! Must reload from storage
    let store2 = PersistentCheckpointStore::new(storage.clone(), "ns_cache").unwrap();
    let cp_miss = store2.get_checkpoint("cp_cache").await.unwrap().unwrap();
    assert_eq!(cp_miss.seq_no, 100);
    assert!(
        *storage.get_count.lock() > initial_get_count,
        "Cache miss must reload from storage"
    );

    // Subsequent read on store2 -> Now Cache Hit!
    let get_count_after_miss = *storage.get_count.lock();
    let cp_hit2 = store2.get_checkpoint("cp_cache").await.unwrap().unwrap();
    assert_eq!(cp_hit2.seq_no, 100);
    assert_eq!(
        *storage.get_count.lock(),
        get_count_after_miss,
        "Subsequent lookup on store2 must hit cache"
    );
}

/// Test Concurrency: N writers creating checkpoints, M readers querying concurrently.
#[tokio::test]
async fn test_concurrent_stress_read_write() {
    let storage = Arc::new(TrackingMockStorage::new());
    let store = Arc::new(PersistentCheckpointStore::new(storage, "ns_stress").unwrap());

    let num_writers = 20;
    let num_readers = 30;

    let mut set: JoinSet<Result<()>> = JoinSet::new();

    // Spawn N writers creating unique checkpoints
    for i in 0..num_writers {
        let store_clone = Arc::clone(&store);
        set.spawn(async move {
            let cp_name = format!("stress_cp_{i}");
            let _ = store_clone
                .create_checkpoint(
                    &cp_name,
                    "col_stress",
                    i as u64,
                    TxId::new(i as u64 + 1),
                    serde_json::json!({"i": i}),
                )
                .await?;
            Ok(())
        });
    }

    // Spawn M readers attempting to list or read checkpoints concurrently
    for i in 0..num_readers {
        let store_clone = Arc::clone(&store);
        set.spawn(async move {
            if i % 2 == 0 {
                let _ = store_clone.list_checkpoints().await?;
            } else {
                let target_name = format!("stress_cp_{}", i % num_writers);
                let _ = store_clone.get_checkpoint(&target_name).await?;
            }
            Ok(())
        });
    }

    // Await all tasks and verify no panics or corruptions
    while let Some(res) = set.join_next().await {
        let task_res = res.expect("Task must not panic");
        assert!(
            task_res.is_ok(),
            "Operation failed under concurrency: {:?}",
            task_res.err()
        );
    }

    // Verify all writer checkpoints were saved intact
    let final_list = store.list_checkpoints().await.unwrap();
    assert_eq!(
        final_list.len(),
        num_writers,
        "All writer checkpoints must be present and consistent"
    );
}

/// Test GC & Pinning Lifecycle: verifying `pin_checkpoint` and `unpin_checkpoint` invariants.
#[tokio::test]
async fn test_pinning_and_gc_exclusion_lifecycle() {
    let storage = Arc::new(TrackingMockStorage::new());
    let store = PersistentCheckpointStore::new(storage.clone(), "ns_pin").unwrap();

    // 1. Create Checkpoint A (seq_no: 10)
    store
        .create_checkpoint("cp_a", "col1", 10, TxId::new(1), serde_json::json!({}))
        .await
        .unwrap();

    // Verify seq_no 10 is pinned in storage
    assert!(
        storage.pinned.lock().contains(&10),
        "Sequence number 10 must be pinned upon checkpoint creation"
    );

    // 2. Overwrite Checkpoint A with same name, but new seq_no: 20
    store
        .create_checkpoint("cp_a", "col1", 20, TxId::new(2), serde_json::json!({}))
        .await
        .unwrap();

    // Verify old seq_no 10 was unpinned, and new seq_no 20 is pinned!
    assert!(
        !storage.pinned.lock().contains(&10),
        "Old sequence number 10 must be unpinned after overwrite"
    );
    assert!(
        storage.pinned.lock().contains(&20),
        "New sequence number 20 must be pinned"
    );

    // 3. Explicitly drop checkpoint
    store.drop_checkpoint("cp_a").await.unwrap();

    // Verify seq_no 20 is now unpinned
    assert!(
        !storage.pinned.lock().contains(&20),
        "Sequence number 20 must be unpinned after drop_checkpoint"
    );
    assert!(
        storage.pinned.lock().is_empty(),
        "No pinned checkpoints should remain"
    );
}
