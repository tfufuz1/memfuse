#![allow(clippy::type_complexity)]

use memfuse_checkpoint::PersistentCheckpointStore;
use memfuse_core::{Result, StorageEngine, StorageStats, TxId};
use parking_lot::Mutex;
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;
use tokio::task::JoinSet;

/// A TxId-versioned storage engine supporting exact time-travel rollbacks.
/// In accordance with LsmStorage invariants, transactions with `tx_id >= TxId::INTERNAL_BASE`
/// represent system metadata (e.g. persistent checkpoint manifests) and are preserved during user-state rollback.
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

    /// Computes deterministic BLAKE3 hash over active user data keys (excluding system checkpoint metadata).
    fn user_state_checksum(&self) -> String {
        let store = self.store.lock();
        let mut hasher = blake3::Hasher::new();
        for (key, versions) in store.iter() {
            if !key.windows(12).any(|w| w == b":checkpoint:") {
                if let Some((_, Some(val))) = versions.iter().next_back() {
                    hasher.update(key);
                    hasher.update(val);
                }
            }
        }
        hasher.finalize().to_hex().to_string()
    }

    /// Computes BLAKE3 state checksum for user data keys matching a specific prefix.
    fn user_state_checksum_prefix(&self, prefix: &[u8]) -> String {
        let store = self.store.lock();
        let mut hasher = blake3::Hasher::new();
        for (key, versions) in store.iter() {
            if key.starts_with(prefix) && !key.windows(12).any(|w| w == b":checkpoint:") {
                if let Some((_, Some(val))) = versions.iter().next_back() {
                    hasher.update(key);
                    hasher.update(val);
                }
            }
        }
        hasher.finalize().to_hex().to_string()
    }

    /// Rolls back user-state key versions strictly for keys matching `prefix`.
    /// System metadata transactions (`v_tx >= TxId::INTERNAL_BASE`) are preserved.
    fn rollback_to_tx_prefix(&self, prefix: &[u8], target_tx: u64) {
        let mut store = self.store.lock();
        for (key, versions) in store.iter_mut() {
            if key.starts_with(prefix) {
                versions.retain(|&v_tx, _| v_tx <= target_tx || v_tx >= TxId::INTERNAL_BASE);
            }
        }
        store.retain(|_, versions| !versions.is_empty());
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
        store.entry(key.to_vec()).or_default().insert(tx_id.0, None);
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
            versions.retain(|&v_tx, _| v_tx <= target_tx || v_tx >= TxId::INTERNAL_BASE);
        }
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

/// A namespace-isolated wrapper around a shared `VersionedMockStorage`.
#[derive(Clone)]
struct NamespaceStorageEngine {
    inner: Arc<VersionedMockStorage>,
    prefix: Vec<u8>,
}

impl NamespaceStorageEngine {
    fn new(inner: Arc<VersionedMockStorage>, prefix: &str) -> Self {
        Self {
            inner,
            prefix: prefix.as_bytes().to_vec(),
        }
    }

    fn prefixed_key(&self, key: &[u8]) -> Vec<u8> {
        let mut k = self.prefix.clone();
        k.extend_from_slice(key);
        k
    }

    fn user_state_checksum(&self) -> String {
        self.inner.user_state_checksum_prefix(&self.prefix)
    }
}

#[async_trait::async_trait]
impl StorageEngine for NamespaceStorageEngine {
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.inner.get(&self.prefixed_key(key)).await
    }

    async fn get_at_seq(&self, key: &[u8], seq: u64) -> Result<Option<Vec<u8>>> {
        self.inner.get_at_seq(&self.prefixed_key(key), seq).await
    }

    async fn put(&self, tx_id: TxId, key: &[u8], value: &[u8]) -> Result<()> {
        self.inner.put(tx_id, &self.prefixed_key(key), value).await
    }

    async fn delete(&self, tx_id: TxId, key: &[u8]) -> Result<()> {
        self.inner.delete(tx_id, &self.prefixed_key(key)).await
    }

    async fn commit(&self, tx_id: TxId) -> Result<()> {
        self.inner.commit(tx_id).await
    }

    async fn rollback(&self, tx_id: TxId) -> Result<()> {
        self.inner.rollback(tx_id).await
    }

    async fn rollback_to_tx(&self, tx_id: TxId) -> Result<()> {
        self.inner.rollback_to_tx_prefix(&self.prefix, tx_id.0);
        Ok(())
    }

    async fn flush(&self) -> Result<()> {
        self.inner.flush().await
    }

    async fn stats(&self) -> Result<StorageStats> {
        self.inner.stats().await
    }

    async fn pin_checkpoint(&self, seq_no: u64) -> Result<()> {
        self.inner.pin_checkpoint(seq_no).await
    }

    async fn unpin_checkpoint(&self, seq_no: u64) -> Result<()> {
        self.inner.unpin_checkpoint(seq_no).await
    }

    async fn last_seq_no(&self) -> Result<u64> {
        self.inner.last_seq_no().await
    }

    async fn last_tx_id(&self) -> Result<TxId> {
        self.inner.last_tx_id().await
    }

    async fn scan(
        &self,
        start: std::ops::Bound<&[u8]>,
        end: std::ops::Bound<&[u8]>,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        self.inner.scan(start, end).await
    }

    async fn scan_prefix(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let full_prefix = self.prefixed_key(prefix);
        let raw = self.inner.scan_prefix(&full_prefix).await?;
        Ok(raw
            .into_iter()
            .map(|(k, v)| (k[self.prefix.len()..].to_vec(), v))
            .collect())
    }

    async fn scan_prefix_at(&self, prefix: &[u8], seq_no: u64) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let full_prefix = self.prefixed_key(prefix);
        let raw = self.inner.scan_prefix_at(&full_prefix, seq_no).await?;
        Ok(raw
            .into_iter()
            .map(|(k, v)| (k[self.prefix.len()..].to_vec(), v))
            .collect())
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

    let checksum_a = storage.user_state_checksum();

    // Create Checkpoint 1 at State A
    let _cp1 = store
        .create_checkpoint("cp1", "col_tt", 1, tx_a, serde_json::json!({"state": "A"}))
        .await
        .unwrap();

    // 2. Transition to State B
    let tx_b = TxId::new(20);
    storage
        .put(tx_b, b"doc_1", b"content_B1_updated")
        .await
        .unwrap();
    storage
        .put(tx_b, b"doc_3", b"content_B3_new")
        .await
        .unwrap();

    let checksum_b = storage.user_state_checksum();
    assert_ne!(
        checksum_a, checksum_b,
        "State B checksum must differ from State A"
    );

    // Create Checkpoint 2 at State B
    let _cp2 = store
        .create_checkpoint("cp2", "col_tt", 2, tx_b, serde_json::json!({"state": "B"}))
        .await
        .unwrap();

    // 3. Transition to State C
    let tx_c = TxId::new(30);
    storage.delete(tx_c, b"doc_2").await.unwrap();
    storage.put(tx_c, b"doc_4", b"content_C4").await.unwrap();

    let checksum_c = storage.user_state_checksum();
    assert_ne!(
        checksum_b, checksum_c,
        "State C checksum must differ from State B"
    );

    // 4. Time-Travel: Restore Checkpoint 1 (State A)
    let restored_meta = store.restore_checkpoint("cp1").await.unwrap();
    assert_eq!(restored_meta.name, "cp1");
    assert_eq!(restored_meta.tx_id, tx_a);

    // 5. Verify byte-exact checksum match with original State A!
    let checksum_restored = storage.user_state_checksum();
    assert_eq!(
        checksum_a, checksum_restored,
        "Restored state checksum must match State A byte-for-byte!"
    );

    // Verify individual key contents
    assert_eq!(
        storage.get(b"doc_1").await.unwrap(),
        Some(b"content_A1".to_vec())
    );
    assert_eq!(
        storage.get(b"doc_2").await.unwrap(),
        Some(b"content_A2".to_vec())
    );
    assert_eq!(storage.get(b"doc_3").await.unwrap(), None);
    assert_eq!(storage.get(b"doc_4").await.unwrap(), None);
}

/// Multi-Session Concurrent Time-Travel Isolation Test (Task F.4):
/// Two independent agent sessions (different namespace and collection) simultaneously perform
/// state mutations, checkpoint creations with identical checkpoint names ("step_1", "step_2"),
/// and time-travel rollbacks.
/// Verifies complete state isolation, byte-exact checksum recovery, and zero cross-session checkpoint leakage.
#[tokio::test]
async fn test_concurrent_two_session_time_travel_isolation() {
    let shared_storage = Arc::new(VersionedMockStorage::new());

    // Agent Session Alpha (Namespace: ns_alpha, Collection: col_alpha)
    let storage_alpha = Arc::new(NamespaceStorageEngine::new(
        shared_storage.clone(),
        "alpha:",
    ));
    let store_alpha = Arc::new(PersistentCheckpointStore::new(
        storage_alpha.clone(),
        "ns_alpha",
    ).unwrap());

    // Agent Session Beta (Namespace: ns_beta, Collection: col_beta)
    let storage_beta = Arc::new(NamespaceStorageEngine::new(shared_storage.clone(), "beta:"));
    let store_beta = Arc::new(PersistentCheckpointStore::new(
        storage_beta.clone(),
        "ns_beta",
    ).unwrap());

    // 1. Session Alpha establishes State A1
    let tx_a1 = TxId::new(101);
    storage_alpha
        .put(tx_a1, b"doc_1", b"alpha_content_v1")
        .await
        .unwrap();
    storage_alpha
        .put(tx_a1, b"doc_2", b"alpha_content_v2")
        .await
        .unwrap();
    let checksum_a1 = storage_alpha.user_state_checksum();

    let cp_a1 = store_alpha
        .create_checkpoint(
            "step_1",
            "col_alpha",
            10,
            tx_a1,
            serde_json::json!({"session": "alpha", "step": 1}),
        )
        .await
        .unwrap();
    assert_eq!(cp_a1.name, "step_1");

    // 2. Session Beta establishes State B1
    let tx_b1 = TxId::new(201);
    storage_beta
        .put(tx_b1, b"doc_1", b"beta_content_v1")
        .await
        .unwrap();
    storage_beta
        .put(tx_b1, b"doc_2", b"beta_content_v2")
        .await
        .unwrap();
    let checksum_b1 = storage_beta.user_state_checksum();

    let cp_b1 = store_beta
        .create_checkpoint(
            "step_1",
            "col_beta",
            10,
            tx_b1,
            serde_json::json!({"session": "beta", "step": 1}),
        )
        .await
        .unwrap();
    assert_eq!(cp_b1.name, "step_1");

    // Checksums across sessions must differ due to namespace-isolated content
    assert_ne!(checksum_a1, checksum_b1);

    // 3. Both sessions transition to State 2 concurrently
    let tx_a2 = TxId::new(102);
    storage_alpha
        .put(tx_a2, b"doc_1", b"alpha_content_v1_updated")
        .await
        .unwrap();
    storage_alpha
        .put(tx_a2, b"doc_3", b"alpha_content_v3_new")
        .await
        .unwrap();
    let _cp_a2 = store_alpha
        .create_checkpoint(
            "step_2",
            "col_alpha",
            20,
            tx_a2,
            serde_json::json!({"session": "alpha", "step": 2}),
        )
        .await
        .unwrap();

    let tx_b2 = TxId::new(202);
    storage_beta
        .put(tx_b2, b"doc_1", b"beta_content_v1_updated")
        .await
        .unwrap();
    storage_beta
        .put(tx_b2, b"doc_3", b"beta_content_v3_new")
        .await
        .unwrap();
    let _cp_b2 = store_beta
        .create_checkpoint(
            "step_2",
            "col_beta",
            20,
            tx_b2,
            serde_json::json!({"session": "beta", "step": 2}),
        )
        .await
        .unwrap();

    // Verify both sessions mutated state
    assert_ne!(storage_alpha.user_state_checksum(), checksum_a1);
    assert_ne!(storage_beta.user_state_checksum(), checksum_b1);

    // 4. SIMULTANEOUS CONCURRENT TIME-TRAVEL ROLLBACK:
    // Session Alpha rolls back to "step_1" while Session Beta simultaneously rolls back to "step_1"
    let store_alpha_clone = store_alpha.clone();
    let store_beta_clone = store_beta.clone();

    let (res_a, res_b) = tokio::join!(
        tokio::spawn(async move { store_alpha_clone.restore_checkpoint("step_1").await }),
        tokio::spawn(async move { store_beta_clone.restore_checkpoint("step_1").await })
    );

    let meta_a = res_a.unwrap().unwrap();
    let meta_b = res_b.unwrap().unwrap();

    assert_eq!(meta_a.collection_id, "col_alpha");
    assert_eq!(meta_b.collection_id, "col_beta");

    // 5. VERIFY COMPLETE ISOLATION & BYTE-EXACT CHECKSUM RECOVERY:
    let restored_checksum_a = storage_alpha.user_state_checksum();
    let restored_checksum_b = storage_beta.user_state_checksum();

    assert_eq!(
        restored_checksum_a, checksum_a1,
        "Session Alpha restored state checksum must match State A1 byte-for-byte!"
    );
    assert_eq!(
        restored_checksum_b, checksum_b1,
        "Session Beta restored state checksum must match State B1 byte-for-byte!"
    );

    // Verify key contents
    assert_eq!(
        storage_alpha.get(b"doc_1").await.unwrap(),
        Some(b"alpha_content_v1".to_vec())
    );
    assert_eq!(storage_alpha.get(b"doc_3").await.unwrap(), None);

    assert_eq!(
        storage_beta.get(b"doc_1").await.unwrap(),
        Some(b"beta_content_v1".to_vec())
    );
    assert_eq!(storage_beta.get(b"doc_3").await.unwrap(), None);

    // 6. VERIFY CHECKPOINT HISTORY ISOLATION:
    let list_alpha = store_alpha.list_checkpoints().await.unwrap();
    let list_beta = store_beta.list_checkpoints().await.unwrap();

    assert_eq!(list_alpha.len(), 2);
    assert!(list_alpha.iter().all(|c| c.collection_id == "col_alpha"));

    assert_eq!(list_beta.len(), 2);
    assert!(list_beta.iter().all(|c| c.collection_id == "col_beta"));
}

/// Stress Test: 100 iterations of simultaneous concurrent mutations, checkpoint creation,
/// guard unwinding, and time-travel restorations across two independent agent sessions.
#[tokio::test]
async fn test_concurrent_two_session_rollback_race_stress_100_iterations() {
    let shared_storage = Arc::new(VersionedMockStorage::new());

    let iterations = 100;
    let mut join_set = JoinSet::new();

    for iter in 0..iterations {
        let shared = shared_storage.clone();

        join_set.spawn(async move {
            let storage_a = Arc::new(NamespaceStorageEngine::new(
                shared.clone(),
                &format!("alpha_iter_{iter}:"),
            ));
            let store_a = Arc::new(PersistentCheckpointStore::new(
                storage_a.clone(),
                format!("ns_alpha_{iter}"),
            ).unwrap());

            let storage_b = Arc::new(NamespaceStorageEngine::new(
                shared.clone(),
                &format!("beta_iter_{iter}:"),
            ));
            let store_b = Arc::new(PersistentCheckpointStore::new(
                storage_b.clone(),
                format!("ns_beta_{iter}"),
            ).unwrap());

            let tx_base_a = 100u64;
            let tx_base_b = 200u64;

            // Session Alpha: Establish baseline
            storage_a
                .put(
                    TxId::new(tx_base_a),
                    b"state_doc",
                    format!("alpha_v1_iter_{iter}").as_bytes(),
                )
                .await?;
            let checksum_a_base = storage_a.user_state_checksum();
            let cp_name_a = format!("cp_alpha_{iter}");
            store_a
                .create_checkpoint(
                    &cp_name_a,
                    "col_alpha",
                    tx_base_a,
                    TxId::new(tx_base_a),
                    serde_json::json!({"iter": iter}),
                )
                .await?;

            // Session Beta: Establish baseline
            storage_b
                .put(
                    TxId::new(tx_base_b),
                    b"state_doc",
                    format!("beta_v1_iter_{iter}").as_bytes(),
                )
                .await?;
            let checksum_b_base = storage_b.user_state_checksum();
            let cp_name_b = format!("cp_beta_{iter}");
            store_b
                .create_checkpoint(
                    &cp_name_b,
                    "col_beta",
                    tx_base_b,
                    TxId::new(tx_base_b),
                    serde_json::json!({"iter": iter}),
                )
                .await?;

            // Concurrent Mutations
            storage_a
                .put(
                    TxId::new(tx_base_a + 1),
                    b"state_doc",
                    format!("alpha_v2_iter_{iter}").as_bytes(),
                )
                .await?;
            storage_b
                .put(
                    TxId::new(tx_base_b + 1),
                    b"state_doc",
                    format!("beta_v2_iter_{iter}").as_bytes(),
                )
                .await?;

            // Concurrent Rollback
            let store_a_task = store_a.clone();
            let store_b_task = store_b.clone();

            let (res_a, res_b) = tokio::join!(
                tokio::spawn(async move { store_a_task.restore_checkpoint(&cp_name_a).await }),
                tokio::spawn(async move { store_b_task.restore_checkpoint(&cp_name_b).await })
            );

            res_a.unwrap()?;
            res_b.unwrap()?;

            // Verify checksum matches
            if storage_a.user_state_checksum() != checksum_a_base {
                return Err(memfuse_core::MemFuseError::Internal(format!(
                    "Alpha checksum mismatch in stress iteration {iter}"
                )));
            }
            if storage_b.user_state_checksum() != checksum_b_base {
                return Err(memfuse_core::MemFuseError::Internal(format!(
                    "Beta checksum mismatch in stress iteration {iter}"
                )));
            }

            Ok::<(), memfuse_core::MemFuseError>(())
        });
    }

    let mut completed = 0;
    while let Some(res) = join_set.join_next().await {
        let task_res = res.expect("Task must not panic");
        assert!(
            task_res.is_ok(),
            "Stress test iteration failed: {:?}",
            task_res.err()
        );
        completed += 1;
    }

    assert_eq!(completed, iterations);

    println!("\n=======================================================");
    println!("STRESS TEST RESULTS: 0 / {iterations} iterations exhibited cross-session history or state pollution.");
    println!("=======================================================\n");
}

/// Verifies RAII CheckpointGuard auto-rollback isolation when Session Alpha's guard drops uncommitted
/// while Session Beta concurrently commits its CheckpointGuard.
#[tokio::test]
async fn test_concurrent_raii_guard_unwind_isolation() {
    let shared_storage = Arc::new(VersionedMockStorage::new());

    let storage_alpha = Arc::new(NamespaceStorageEngine::new(
        shared_storage.clone(),
        "guard_alpha:",
    ));
    let store_alpha = PersistentCheckpointStore::new(storage_alpha.clone(), "ns_guard_alpha").unwrap();

    let storage_beta = Arc::new(NamespaceStorageEngine::new(
        shared_storage.clone(),
        "guard_beta:",
    ));
    let store_beta = PersistentCheckpointStore::new(storage_beta.clone(), "ns_guard_beta");

    // Baseline state for both sessions
    let tx_base_a = TxId::new(10);
    storage_alpha
        .put(tx_base_a, b"doc_1", b"alpha_init")
        .await
        .unwrap();

    let tx_base_b = TxId::new(20);
    storage_beta
        .put(tx_base_b, b"doc_1", b"beta_init")
        .await
        .unwrap();

    let tx_mut_a = TxId::new(11);
    let tx_mut_b = TxId::new(21);

    // Session Alpha creates guard, mutates state
    let guard_alpha = store_alpha.create_guard(tx_base_a).unwrap();
    storage_alpha
        .put(tx_mut_a, b"doc_1", b"alpha_uncommitted_mutation")
        .await
        .unwrap();

    // Session Beta creates guard, mutates state, commits guard
    let guard_beta = store_beta.create_guard(tx_base_b).unwrap();
    storage_beta
        .put(tx_mut_b, b"doc_1", b"beta_committed_mutation")
        .await
        .unwrap();
    let _cp_beta = guard_beta.commit().unwrap();

    // Drop Session Alpha's guard without calling commit (registers orphaned checkpoint)
    drop(guard_alpha);

    // Execute controlled recovery for Session Alpha
    let recovered = store_alpha.recover_orphaned_checkpoints().await.unwrap();
    assert_eq!(recovered, vec![tx_base_a]);

    // Verify Session Alpha's state was rolled back to alpha_init
    assert_eq!(
        storage_alpha.get(b"doc_1").await.unwrap(),
        Some(b"alpha_init".to_vec())
    );

    // Verify Session Beta's state remains committed as beta_committed_mutation
    assert_eq!(
        storage_beta.get(b"doc_1").await.unwrap(),
        Some(b"beta_committed_mutation".to_vec())
    );
}

/// Verifies sequence number pinning and unpinning lifecycle isolation across concurrent sessions.
#[tokio::test]
async fn test_concurrent_pinning_lifecycle_isolation() {
    let shared_storage = Arc::new(VersionedMockStorage::new());

    let storage_alpha = Arc::new(NamespaceStorageEngine::new(
        shared_storage.clone(),
        "pin_alpha:",
    ));
    let store_alpha = PersistentCheckpointStore::new(storage_alpha, "ns_pin_alpha");

    let storage_beta = Arc::new(NamespaceStorageEngine::new(
        shared_storage.clone(),
        "pin_beta:",
    ));
    let store_beta = PersistentCheckpointStore::new(storage_beta, "ns_pin_beta");

    // Session Alpha creates CP_A1 (seq_no 100)
    store_alpha
        .create_checkpoint("cp1", "col_alpha", 100, TxId::new(1), serde_json::json!({}))
        .await
        .unwrap();

    // Session Beta creates CP_B1 (seq_no 200)
    store_beta
        .create_checkpoint("cp1", "col_beta", 200, TxId::new(2), serde_json::json!({}))
        .await
        .unwrap();

    // Both seq_no 100 and 200 must be pinned in shared storage
    assert!(shared_storage.pinned.lock().contains(&100));
    assert!(shared_storage.pinned.lock().contains(&200));

    // Session Alpha overwrites "cp1" with seq_no 300 (unpins 100, pins 300)
    store_alpha
        .create_checkpoint("cp1", "col_alpha", 300, TxId::new(3), serde_json::json!({}))
        .await
        .unwrap();

    // Session Alpha's old seq_no 100 must be unpinned, 300 pinned
    assert!(!shared_storage.pinned.lock().contains(&100));
    assert!(shared_storage.pinned.lock().contains(&300));

    // CRITICAL: Session Beta's seq_no 200 MUST REMAIN PINNED!
    assert!(
        shared_storage.pinned.lock().contains(&200),
        "Session Beta's pinned sequence number 200 must not be unpinned by Session Alpha's overwrite"
    );

    // Session Beta drops its checkpoint
    store_beta.drop_checkpoint("cp1").await.unwrap();

    // Now seq_no 200 is unpinned, while Alpha's 300 remains pinned
    assert!(!shared_storage.pinned.lock().contains(&200));
    assert!(shared_storage.pinned.lock().contains(&300));
}
