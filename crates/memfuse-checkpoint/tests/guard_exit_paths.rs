use memfuse_checkpoint::{CheckpointGuard, PersistentCheckpointStore};
use memfuse_core::{Result, StorageEngine, StorageStats, TxId};
use parking_lot::Mutex;
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

/// Scenario A: Normal Drop after explicit .commit() call — verify final state is persisted / no rollback triggered.
#[tokio::test]
async fn test_guard_exit_path_a_normal_commit_drop() {
    let storage = Arc::new(TrackingMockStorage::new());
    let store = PersistentCheckpointStore::new(storage.clone(), "test_a");

    let guard = store.create_guard(TxId::new(101)).unwrap();
    let cp = guard.commit().unwrap();
    assert_eq!(cp.tx_id, TxId::new(101));

    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    assert!(
        storage.rolled_back_txs.lock().is_empty(),
        "Committed guard must not trigger auto-rollback on drop"
    );
}

/// Scenario B: Drop WITHOUT explicit commit (e.g. scope end or `?` error propagation) — verify auto-rollback triggers reliably.
#[tokio::test]
async fn test_guard_exit_path_b_uncommitted_drop_triggers_rollback() {
    let storage = Arc::new(TrackingMockStorage::new());
    let store = PersistentCheckpointStore::new(storage.clone(), "test_b");

    {
        let _guard = store.create_guard(TxId::new(202)).unwrap();
        // Scope ends without calling .commit()
    }

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let rolled_back = storage.rolled_back_txs.lock().clone();
    assert_eq!(
        rolled_back,
        vec![TxId::new(202)],
        "Uncommitted guard drop must trigger auto-rollback to TxId 202"
    );
}

/// Scenario C: Drop during a Panic (Panic-Unwind path) — verify state is rolled back cleanly.
#[tokio::test]
async fn test_guard_exit_path_c_panic_unwind_triggers_rollback() {
    let storage = Arc::new(TrackingMockStorage::new());
    let store = PersistentCheckpointStore::new(storage.clone(), "test_c");

    let storage_task = storage.clone();
    let store_task = store;

    let handle = tokio::spawn(async move {
        let _guard = store_task.create_guard(TxId::new(303)).unwrap();
        // Deliberately trigger panic inside guard scope
        panic!("Simulated agent failure / unhandled panic during processing");
    });

    // Catch task join error due to panic
    let result = handle.await;
    assert!(result.is_err(), "Task should have panicked");
    assert!(
        result.unwrap_err().is_panic(),
        "JoinError must represent a panic"
    );

    // Give background tokio task spawned in Drop time to run
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let rolled_back = storage_task.rolled_back_txs.lock().clone();
    assert_eq!(
        rolled_back,
        vec![TxId::new(303)],
        "Panic unwind must drop guard and execute auto-rollback to TxId 303"
    );
}

/// Scenario D: Explicit .rollback() call followed by drop — verify idempotency and no double-rollback error.
#[tokio::test]
async fn test_guard_exit_path_d_explicit_rollback_and_drop_idempotent() {
    let storage = Arc::new(TrackingMockStorage::new());
    let store = PersistentCheckpointStore::new(storage.clone(), "test_d");

    let guard = store.create_guard(TxId::new(404)).unwrap();

    // Explicitly call .rollback().await
    let rollback_res = guard.rollback().await;
    assert!(
        rollback_res.is_ok(),
        "Explicit rollback on active guard must succeed"
    );

    // Verify storage received the rollback immediately
    assert_eq!(
        storage.rolled_back_txs.lock().clone(),
        vec![TxId::new(404)]
    );

    // Now let `guard` drop here. Check that drop does NOT attempt a second rollback.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert_eq!(
        storage.rolled_back_txs.lock().len(),
        1,
        "Drop after explicit rollback must be idempotent and not issue duplicate rollback"
    );
}

/// Scenario E: Nested Guards — Guard inside another Guard scope — test LIFO resolution.
#[tokio::test]
async fn test_guard_exit_path_e_nested_guards_lifo_resolution() {
    let storage = Arc::new(TrackingMockStorage::new());
    let store = PersistentCheckpointStore::new(storage.clone(), "test_e");

    {
        let _outer_guard = store.create_guard(TxId::new(501)).unwrap();
        {
            let _inner_guard = store.create_guard(TxId::new(502)).unwrap();
            // Inner guard drops first
        }
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        let rolled_back_so_far = storage.rolled_back_txs.lock().clone();
        assert_eq!(
            rolled_back_so_far,
            vec![TxId::new(502)],
            "Inner guard must auto-rollback first"
        );
        // Outer guard drops second
    }

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let final_rolled_back = storage.rolled_back_txs.lock().clone();
    assert_eq!(
        final_rolled_back,
        vec![TxId::new(502), TxId::new(501)],
        "Nested guards must unwind and execute rollbacks in strict LIFO order"
    );
}

/// End-to-End Agent Step Cycle via `for_agent_step()`
#[tokio::test]
async fn test_for_agent_step_e2e_cycle() {
    let storage = Arc::new(TrackingMockStorage::new());

    // 1. Begin agent step
    let guard = CheckpointGuard::for_agent_step(storage.clone(), TxId::new(601))
        .await
        .unwrap();

    assert_eq!(guard.checkpoint().unwrap().tx_id, TxId::new(601));
    assert!(guard.checkpoint().unwrap().timestamp_ms > 0);

    // 2. Perform agent work (mock)
    storage
        .put(TxId::new(601), b"agent_key", b"agent_value")
        .await
        .unwrap();

    // 3. Commit agent step successfully
    let state = guard.commit().unwrap();
    assert_eq!(state.tx_id, TxId::new(601));

    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    assert!(
        storage.rolled_back_txs.lock().is_empty(),
        "Agent step committed successfully should not trigger rollback"
    );
}
