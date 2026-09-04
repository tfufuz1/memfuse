#![allow(clippy::await_holding_lock, deprecated)]

use memfuse_checkpoint::{CheckpointGuard, PersistentCheckpointStore};
use memfuse_core::{MemFuseError, Result, StorageEngine, StorageStats, TxId};
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

static TEST_LOCK: Mutex<()> = Mutex::new(());

struct TrackingMockStorage {
    data: Mutex<HashMap<Vec<u8>, Vec<u8>>>,
    pinned: Mutex<HashSet<u64>>,
    rolled_back_txs: Mutex<Vec<TxId>>,
    last_tx: Mutex<TxId>,
}

impl TrackingMockStorage {
    fn new() -> Self {
        Self {
            data: Mutex::new(HashMap::new()),
            pinned: Mutex::new(HashSet::new()),
            rolled_back_txs: Mutex::new(Vec::new()),
            last_tx: Mutex::new(TxId::new(0)),
        }
    }
}

#[async_trait::async_trait]
impl StorageEngine for TrackingMockStorage {
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        Ok(self.data.lock().get(key).cloned())
    }
    async fn put(&self, tx_id: TxId, key: &[u8], value: &[u8]) -> Result<()> {
        self.data.lock().insert(key.to_vec(), value.to_vec());
        let mut last = self.last_tx.lock();
        if tx_id > *last {
            *last = tx_id;
        }
        Ok(())
    }
    async fn delete(&self, _tx_id: TxId, key: &[u8]) -> Result<()> {
        self.data.lock().remove(key);
        Ok(())
    }
    async fn commit(&self, tx_id: TxId) -> Result<()> {
        let mut last = self.last_tx.lock();
        if tx_id > *last {
            *last = tx_id;
        }
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
        Ok(*self.last_tx.lock())
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
    let _lock = TEST_LOCK.lock();
    let storage = Arc::new(TrackingMockStorage::new());
    let store = PersistentCheckpointStore::new(storage.clone(), "test_a");

    let guard = store.create_guard(TxId::new(101)).unwrap();
    let cp = guard.commit().unwrap();
    assert_eq!(cp.tx_id, TxId::new(101));

    assert!(
        storage.rolled_back_txs.lock().is_empty(),
        "Committed guard must not trigger auto-rollback on drop"
    );
}

/// Scenario B: Drop WITHOUT explicit commit (e.g. scope end or `?` error propagation) — registers orphaned checkpoint, recovered in startup/recovery cycle.
#[tokio::test]
async fn test_guard_exit_path_b_uncommitted_drop_triggers_rollback() {
    let _lock = TEST_LOCK.lock();
    let storage = Arc::new(TrackingMockStorage::new());
    let store = PersistentCheckpointStore::new(storage.clone(), "test_b");

    {
        let _guard = store.create_guard(TxId::new(202)).unwrap();
        // Scope ends without calling .commit()
    }

    assert_eq!(store.get_orphaned_checkpoints().len(), 1);

    // Perform controlled startup recovery
    let recovered = store.recover_orphaned_checkpoints().await.unwrap();
    assert_eq!(recovered, vec![TxId::new(202)]);

    let rolled_back = storage.rolled_back_txs.lock().clone();
    assert_eq!(
        rolled_back,
        vec![TxId::new(202)],
        "Controlled recovery must process orphaned checkpoint and execute rollback to TxId 202"
    );
}

/// Scenario C: Drop during a Panic (Panic-Unwind path) — registers orphaned checkpoint, recovered in controlled recovery cycle.
#[tokio::test]
async fn test_guard_exit_path_c_panic_unwind_triggers_rollback() {
    let _lock = TEST_LOCK.lock();
    let storage = Arc::new(TrackingMockStorage::new());
    let store = Arc::new(PersistentCheckpointStore::new(storage.clone(), "test_c"));

    let store_task = store.clone();

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

    assert_eq!(store.get_orphaned_checkpoints().len(), 1);

    // Controlled startup recovery executes the rollback for orphaned checkpoint
    let recovered = store.recover_orphaned_checkpoints().await.unwrap();
    assert_eq!(recovered, vec![TxId::new(303)]);

    let rolled_back = storage.rolled_back_txs.lock().clone();
    assert_eq!(
        rolled_back,
        vec![TxId::new(303)],
        "Controlled recovery after panic unwind must execute auto-rollback to TxId 303"
    );
}

/// Scenario D: Explicit .rollback() call followed by drop — verify idempotency and no double-rollback error.
#[tokio::test]
async fn test_guard_exit_path_d_explicit_rollback_and_drop_idempotent() {
    let _lock = TEST_LOCK.lock();
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
    assert_eq!(storage.rolled_back_txs.lock().clone(), vec![TxId::new(404)]);

    // Now let `guard` drop here. Check that drop does NOT register an orphaned checkpoint or issue duplicate rollback.
    assert_eq!(
        storage.rolled_back_txs.lock().len(),
        1,
        "Drop after explicit rollback must be idempotent and not issue duplicate rollback"
    );
    assert_eq!(store.get_orphaned_checkpoints().len(), 0);
}

/// Scenario E: Nested Guards — Guard inside another Guard scope — test LIFO resolution during controlled recovery.
#[tokio::test]
async fn test_guard_exit_path_e_nested_guards_lifo_resolution() {
    let _lock = TEST_LOCK.lock();
    let storage = Arc::new(TrackingMockStorage::new());
    let store = PersistentCheckpointStore::new(storage.clone(), "test_e");

    {
        let _outer_guard = store.create_guard(TxId::new(501)).unwrap();
        {
            let _inner_guard = store.create_guard(TxId::new(502)).unwrap();
            // Inner guard drops first
        }
        // Outer guard drops second
    }

    assert_eq!(store.get_orphaned_checkpoints().len(), 2);

    let recovered = store.recover_orphaned_checkpoints().await.unwrap();
    assert_eq!(recovered, vec![TxId::new(502), TxId::new(501)]);
}

/// End-to-End Agent Step Cycle via `for_agent_step()`
#[tokio::test]
async fn test_for_agent_step_e2e_cycle() {
    let _lock = TEST_LOCK.lock();
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

    assert!(
        storage.rolled_back_txs.lock().is_empty(),
        "Agent step committed successfully should not trigger rollback"
    );
}

/// MANDATORY TEST 1:
/// Guard wird ohne Commit gedroppt, während parallel eine Transaktion mit höherer TxId committet.
/// Verifiziere, dass die neuere Transaktion NICHT durch den Rollback zerstört wird
/// (Rollback wird durch Serialisierungsbarriere abgelehnt bzw. während Recovery übersprungen).
#[tokio::test]
async fn test_guard_uncommitted_drop_with_newer_committed_tx_preserves_newer_tx() {
    let _lock = TEST_LOCK.lock();
    let storage = Arc::new(TrackingMockStorage::new());
    let store = PersistentCheckpointStore::new(storage.clone(), "test_barrier");

    // Session Alpha creates guard at TxId 100
    let guard_alpha = store.create_guard(TxId::new(100)).unwrap();
    storage
        .put(TxId::new(100), b"alpha_key", b"alpha_val")
        .await
        .unwrap();

    // Session Beta concurrently performs & commits transaction at TxId 200 (> 100)
    storage
        .put(TxId::new(200), b"beta_key", b"beta_val")
        .await
        .unwrap();
    storage.commit(TxId::new(200)).await.unwrap();

    // Session Alpha drops uncommitted guard
    drop(guard_alpha);

    // Verify orphaned checkpoint registered
    assert_eq!(store.get_orphaned_checkpoints().len(), 1);

    // Attempt controlled recovery:
    // Because last_tx is 200 (> 100), the serialization barrier prevents rolling back to TxId 100!
    let recovered = store.recover_orphaned_checkpoints().await.unwrap();
    assert!(
        recovered.is_empty(),
        "Orphaned rollback must be skipped due to serialization barrier (last_tx = 200 > target_tx = 100)"
    );

    // Verify the newer transaction's state is preserved intact
    assert_eq!(
        storage.get(b"beta_key").await.unwrap(),
        Some(b"beta_val".to_vec()),
        "Newer committed transaction at TxId 200 must NOT be destroyed by late rollback of TxId 100"
    );

    // Explicit call to guard_alpha.rollback() would also fail with serialization barrier error
    let guard_alpha2 = store.create_guard(TxId::new(100)).unwrap();
    let res = guard_alpha2.rollback().await;
    assert!(
        res.is_err(),
        "Explicit rollback must fail serialization barrier check when newer tx exists"
    );
    if let Err(MemFuseError::Transaction(msg)) = res {
        assert!(msg.contains("Serialization barrier violation"));
    } else {
        panic!("Expected MemFuseError::Transaction serialization barrier error");
    }
}

/// MANDATORY TEST 2:
/// Guard wird außerhalb einer Tokio-Runtime gedroppt.
/// Verifiziere, dass ein "orphaned checkpoint"-Eintrag persistiert wird und beim nächsten Hochfahren verarbeitet wird.
#[test]
fn test_guard_dropped_outside_tokio_runtime_persists_orphan_and_recovers_on_startup() {
    let _lock = TEST_LOCK.lock();

    let orphan_file = std::path::PathBuf::from("outside_tokio_orphaned_checkpoints.json");
    if orphan_file.exists() {
        let _ = std::fs::remove_file(&orphan_file);
    }

    // Step 1: Drop guard outside active Tokio runtime
    let thread_handle = std::thread::spawn(|| {
        let storage = Arc::new(TrackingMockStorage::new());
        let store = PersistentCheckpointStore::new(storage, "outside_tokio");

        let _guard = store.create_guard(TxId::new(888)).unwrap();
        // _guard drops here outside any Tokio runtime
    });

    thread_handle
        .join()
        .expect("Thread outside Tokio runtime should complete safely without panic");

    // Step 2: Verify orphaned checkpoint was registered and persisted to disk file
    assert!(
        orphan_file.exists(),
        "Orphaned checkpoints file must be persisted to disk on drop outside Tokio runtime"
    );

    // Step 3: Simulate next controlled application startup with new Tokio runtime
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let storage = Arc::new(TrackingMockStorage::new());
        let store = PersistentCheckpointStore::new(storage.clone(), "outside_tokio");

        // Execute startup recovery
        let recovered = store.recover_orphaned_checkpoints().await.unwrap();
        assert_eq!(
            recovered,
            vec![TxId::new(888)],
            "Orphaned checkpoint persisted outside Tokio runtime must be recovered on startup"
        );

        let rolled_back = storage.rolled_back_txs.lock().clone();
        assert_eq!(rolled_back, vec![TxId::new(888)]);
    });

    if orphan_file.exists() {
        let _ = std::fs::remove_file(&orphan_file);
    }
}
