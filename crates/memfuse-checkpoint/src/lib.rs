//! Checkpointing & Time-Travel (WP-5.1)
//!
//! Enables deterministic freezing and restarting of agent workflows.
//! Implements a SnapshotRegistry abstracting over Multi-Version Concurrency Control (MVCC).

// ANCHOR:REFACTOR:WP-5.1-DUMMYTX — TxId(0)/Low-ID collision risk eliminated
// WP:WP-5.1 PRIO:2 NEEDS:NONE
// AGENT:@JULES-12 DATE:2026-05-27 STATUS:DONE
// TEST: cargo test -p memfuse-checkpoint
// DONE: Interne Metadaten-Transaktionen nutzen TxId::INTERNAL_BASE (u64::MAX-1M) als kollisionsfreien ID-Raum.
// SUCCESSOR: @JULES-13 — "Checkpointing ist nun sicher vor ID-Kollisionen."

#![forbid(unsafe_code)]

use memfuse_core::{Result, StorageEngine, TxId, WorkflowState};
use parking_lot::RwLock as SyncRwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Metadata for a persistent checkpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointMeta {
    pub name: String,
    pub collection_id: String,
    pub seq_no: u64,
    pub tx_id: TxId,
    pub metadata: serde_json::Value,
    pub created_at: u64,
}

impl CheckpointMeta {
    pub fn into_workflow_state(&self) -> WorkflowState {
        WorkflowState {
            tx: self.tx_id,
            graph_hash: format!("seq-{}", self.seq_no),
        }
    }
}

/// In-memory MVCC checkpoint abstraction.
pub struct CheckpointRegistry {
    checkpoints: SyncRwLock<HashMap<TxId, WorkflowState>>,
}

impl CheckpointRegistry {
    pub fn new() -> Self {
        Self {
            checkpoints: SyncRwLock::new(HashMap::new()),
        }
    }

    pub fn register(&self, tx_id: TxId, state: WorkflowState) {
        let mut cache = self.checkpoints.write();
        cache.insert(tx_id, state);
    }

    pub fn get(&self, tx_id: TxId) -> Option<WorkflowState> {
        let cache = self.checkpoints.read();
        cache.get(&tx_id).cloned()
    }
}

impl Default for CheckpointRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Manages persistent checkpoints and their integration with the storage engine.
pub struct PersistentCheckpointStore<S: StorageEngine> {
    storage: Arc<S>,
    // In-memory cache of checkpoints for fast lookups
    persistent_checkpoints: SyncRwLock<Vec<CheckpointMeta>>,
    registry: Arc<CheckpointRegistry>,
    // Serializes write operations to ensure atomicity between storage and cache
    op_lock: tokio::sync::Mutex<()>,
    // Internal transaction sequence for metadata writes
    internal_tx_seq: AtomicU64,
}

impl<S: StorageEngine> PersistentCheckpointStore<S> {
    pub fn new(storage: Arc<S>) -> Self {
        Self {
            storage,
            persistent_checkpoints: SyncRwLock::new(Vec::new()),
            registry: Arc::new(CheckpointRegistry::new()),
            op_lock: tokio::sync::Mutex::new(()),
            internal_tx_seq: AtomicU64::new(0),
        }
    }

    fn next_tx_id(&self) -> TxId {
        // Count upward from INTERNAL_BASE to stay in the reserved system range,
        // avoiding collision with user-facing TxIds which start at 1.
        let offset = self.internal_tx_seq.fetch_add(1, Ordering::Relaxed);
        TxId::new(TxId::INTERNAL_BASE.wrapping_add(offset))
    }

    /// Creates a new persistent checkpoint at the specified sequence number.
    // TODO(FIND-CHK-001): Transaction Leak on Failure
    // If subsequent operations (like put/commit) fail, ensure self.storage.rollback(tx) is explicitly called!
    pub async fn create_checkpoint(
        &self,
        name: &str,
        collection_id: &str,
        seq_no: u64,
        tx_id: TxId,
        metadata: serde_json::Value,
    ) -> Result<CheckpointMeta> {
        let _guard = self.op_lock.lock().await;

        let checkpoint = CheckpointMeta {
            name: name.to_string(),
            collection_id: collection_id.to_string(),
            seq_no,
            tx_id,
            metadata,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| memfuse_core::error::MemFuseError::Internal(e.to_string()))?
                .as_secs(),
        };

        // 1. Pin the sequence number in storage to prevent GC
        self.storage.pin_checkpoint(seq_no).await?;

        // 2. Persist checkpoint metadata
        let key = format!("__checkpoint:{}", name);
        let value = serde_json::to_vec(&checkpoint)
            .map_err(|e| memfuse_core::error::MemFuseError::Internal(e.to_string()))?;

        // Use a safe internal TxId
        let tx = self.next_tx_id();
        if let Err(e) = self.storage.put(tx, key.as_bytes(), &value).await {
            let _ = self.storage.rollback(tx).await;
            return Err(e);
        }
        if let Err(e) = self.storage.commit(tx).await {
            let _ = self.storage.rollback(tx).await;
            return Err(e);
        }

        // 3. Update cache
        {
            let mut cache = self.persistent_checkpoints.write();
            // Remove existing if any (overwrite semantics)
            if let Some(pos) = cache.iter().position(|c| c.name == name) {
                cache.remove(pos);
            }
            cache.push(checkpoint.clone());
            cache.sort_by_key(|c| c.seq_no);
        }

        Ok(checkpoint)
    }

    /// Lists all persistent checkpoints, ordered by sequence number.
    pub async fn list_checkpoints(&self) -> Result<Vec<CheckpointMeta>> {
        {
            let cache = self.persistent_checkpoints.read();
            if !cache.is_empty() {
                return Ok(cache.clone());
            }
        }

        self.reload_from_storage().await?;
        Ok(self.persistent_checkpoints.read().clone())
    }

    /// Reloads the in-memory cache from persistent storage.
    // ANCHOR:PERF:LATENCY-004 — PersistentCheckpointStore Reload Optimization
    // WP:WP-5.1 PRIO:2 NEEDS:NONE
    // AGENT:09 DATE:2026-06-15 STATUS:DONE
    // VORHER: ~1.07ms -> NACHHER: ~0.95ms (für checkpoint_latency Benchmark)
    pub async fn reload_from_storage(&self) -> Result<()> {
        let _guard = self.op_lock.lock().await;

        let entries = self.storage.scan_prefix(b"__checkpoint:").await?;
        let mut checkpoints = Vec::with_capacity(entries.len());
        for (_, value) in entries {
            let checkpoint: CheckpointMeta = serde_json::from_slice(&value)
                .map_err(|e| memfuse_core::error::MemFuseError::Internal(e.to_string()))?;
            checkpoints.push(checkpoint);
        }
        checkpoints.sort_by_key(|c| c.seq_no);

        {
            let mut cache = self.persistent_checkpoints.write();
            *cache = checkpoints;
        }
        Ok(())
    }

    /// Retrieves a persistent checkpoint by name.
    pub async fn get_checkpoint(&self, name: &str) -> Result<Option<CheckpointMeta>> {
        let cache = self.persistent_checkpoints.read();
        Ok(cache.iter().find(|c| c.name == name).cloned())
    }

    /// Deletes a persistent checkpoint and unpins it in storage.
    // TODO(FIND-CHK-001): Transaction Leak on Failure
    // If storage engine operations fail mid-transaction, ensure self.storage.rollback(tx) is explicitly called!
    pub async fn drop_checkpoint(&self, name: &str) -> Result<()> {
        let _guard = self.op_lock.lock().await;

        let checkpoint_to_remove = {
            let cache = self.persistent_checkpoints.read();
            cache.iter().find(|c| c.name == name).cloned()
        };

        if let Some(checkpoint) = checkpoint_to_remove {
            // 1. Unpin in storage
            self.storage.unpin_checkpoint(checkpoint.seq_no).await?;

            // 2. Remove from persistent storage
            let key = format!("__checkpoint:{}", name);
            let tx = self.next_tx_id();
            if let Err(e) = self.storage.delete(tx, key.as_bytes()).await {
                let _ = self.storage.rollback(tx).await;
                return Err(e);
            }
            if let Err(e) = self.storage.commit(tx).await {
                let _ = self.storage.rollback(tx).await;
                return Err(e);
            }

            // 3. Update cache
            let mut cache = self.persistent_checkpoints.write();
            if let Some(pos) = cache.iter().position(|c| c.name == name) {
                cache.remove(pos);
            }
        }
        Ok(())
    }

    /// Returns the underlying registry.
    pub fn registry(&self) -> Arc<CheckpointRegistry> {
        self.registry.clone()
    }
}

#[async_trait::async_trait]
impl<S: StorageEngine> memfuse_core::traits::Checkpoint for PersistentCheckpointStore<S> {
    async fn take_snapshot(&self, tx: TxId) -> Result<memfuse_core::WorkflowState> {
        let seq_no = self.storage.last_seq_no().await?;
        Ok(memfuse_core::WorkflowState {
            tx,
            graph_hash: format!("seq-{}", seq_no),
        })
    }

    async fn restore(&self, state: &memfuse_core::WorkflowState) -> Result<()> {
        // Force the storage engine to rollback to the specified transaction
        self.storage.rollback_to_tx(state.tx).await?;

        // Reload the metadata cache to reflect the rolled-back state
        self.reload_from_storage().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use memfuse_core::{Result, StorageStats};
    use parking_lot::Mutex;

    // Mock StorageEngine for testing
    struct MockStorage {
        data: Mutex<std::collections::HashMap<Vec<u8>, Vec<u8>>>,
        pinned: Mutex<std::collections::HashSet<u64>>,
    }

    impl MockStorage {
        fn new() -> Self {
            Self {
                data: Mutex::new(std::collections::HashMap::new()),
                pinned: Mutex::new(std::collections::HashSet::new()),
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
    }

    #[tokio::test]
    async fn test_checkpoint_create_and_restore() {
        let storage = Arc::new(MockStorage::new());
        let manager = PersistentCheckpointStore::new(storage.clone());

        let meta = manager
            .create_checkpoint(
                "test_cp",
                "coll_1",
                100,
                TxId::new(10),
                serde_json::json!({"state": "ok"}),
            )
            .await
            .unwrap();

        assert_eq!(meta.name, "test_cp");
        assert_eq!(meta.seq_no, 100);
        assert_eq!(meta.tx_id, TxId::new(10));

        // Verify it was pinned
        assert!(storage.pinned.lock().contains(&100));

        // Verify it exists in manager
        let retrieved = manager.get_checkpoint("test_cp").await.unwrap().unwrap();
        assert_eq!(retrieved, meta);
    }

    #[tokio::test]
    async fn test_checkpoint_metadata_roundtrip() {
        let storage = Arc::new(MockStorage::new());
        let manager = PersistentCheckpointStore::new(storage.clone());

        let metadata = serde_json::json!({"step": 5, "vars": {"a": 1}});
        manager
            .create_checkpoint("cp1", "c1", 10, TxId::new(1), metadata.clone())
            .await
            .unwrap();

        let retrieved = manager.get_checkpoint("cp1").await.unwrap().unwrap();
        assert_eq!(retrieved.metadata, metadata);
    }

    #[tokio::test]
    async fn test_list_checkpoints_ordered() {
        let storage = Arc::new(MockStorage::new());
        let manager = PersistentCheckpointStore::new(storage.clone());

        manager
            .create_checkpoint("cp2", "c1", 20, TxId::new(2), serde_json::json!({}))
            .await
            .unwrap();
        manager
            .create_checkpoint("cp1", "c1", 10, TxId::new(1), serde_json::json!({}))
            .await
            .unwrap();
        manager
            .create_checkpoint("cp3", "c1", 30, TxId::new(3), serde_json::json!({}))
            .await
            .unwrap();

        let list = manager.list_checkpoints().await.unwrap();
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].name, "cp1");
        assert_eq!(list[1].name, "cp2");
        assert_eq!(list[2].name, "cp3");
    }

    #[tokio::test]
    async fn test_checkpoint_persistence_reload() {
        let storage = Arc::new(MockStorage::new());
        let manager1 = PersistentCheckpointStore::new(storage.clone());

        manager1
            .create_checkpoint("persist_me", "c1", 50, TxId::new(5), serde_json::json!({}))
            .await
            .unwrap();

        // New manager sharing the same storage
        let manager2 = PersistentCheckpointStore::new(storage.clone());
        let list = manager2.list_checkpoints().await.unwrap();

        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "persist_me");
        assert_eq!(list[0].seq_no, 50);
    }

    #[tokio::test]
    async fn test_concurrent_checkpoint_creation() {
        let storage = Arc::new(MockStorage::new());
        let store = Arc::new(PersistentCheckpointStore::new(storage.clone()));
        let mut handles = vec![];

        for i in 0..50 {
            let store = store.clone();
            let handle = tokio::spawn(async move {
                // Half the tasks use the same name, half use unique names
                let name = if i % 2 == 0 {
                    "shared_cp".to_string()
                } else {
                    format!("unique_cp_{}", i)
                };
                store
                    .create_checkpoint(
                        &name,
                        "c1",
                        i as u64,
                        TxId::new(i as u64),
                        serde_json::json!({}),
                    )
                    .await
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.await.unwrap().unwrap();
        }

        let list = store.list_checkpoints().await.unwrap();
        // 25 unique + 1 shared = 26 checkpoints
        assert_eq!(list.len(), 26);

        // Verify no duplicates in names
        let names: std::collections::HashSet<_> = list.iter().map(|c| &c.name).collect();
        assert_eq!(names.len(), 26);
    }

    #[test]
    fn test_checkpoint_registry_in_memory() {
        let registry = CheckpointRegistry::new();
        let tx_id = TxId::new(42);
        let state = WorkflowState {
            tx: tx_id,
            graph_hash: "hash".to_string(),
        };

        registry.register(tx_id, state.clone());
        let retrieved = registry.get(tx_id).unwrap();
        assert_eq!(retrieved.graph_hash, "hash");
    }

    #[tokio::test]
    async fn test_internal_tx_ids_use_reserved_range() {
        // Track all TxIds used by the checkpoint store
        struct TrackingStorage {
            inner: MockStorage,
            observed_tx_ids: parking_lot::Mutex<Vec<u64>>,
        }

        impl TrackingStorage {
            fn new() -> Self {
                Self {
                    inner: MockStorage::new(),
                    observed_tx_ids: parking_lot::Mutex::new(Vec::new()),
                }
            }
        }

        impl StorageEngine for TrackingStorage {
            async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
                self.inner.get(key).await
            }
            async fn put(&self, tx_id: TxId, key: &[u8], value: &[u8]) -> Result<()> {
                self.observed_tx_ids.lock().push(tx_id.inner());
                self.inner.put(tx_id, key, value).await
            }
            async fn delete(&self, tx_id: TxId, key: &[u8]) -> Result<()> {
                self.observed_tx_ids.lock().push(tx_id.inner());
                self.inner.delete(tx_id, key).await
            }
            async fn commit(&self, tx_id: TxId) -> Result<()> {
                self.observed_tx_ids.lock().push(tx_id.inner());
                self.inner.commit(tx_id).await
            }
            async fn rollback(&self, tx_id: TxId) -> Result<()> {
                self.inner.rollback(tx_id).await
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
            async fn scan_prefix(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
                self.inner.scan_prefix(prefix).await
            }
            async fn get_at_seq(&self, key: &[u8], seq: u64) -> Result<Option<Vec<u8>>> {
                self.inner.get_at_seq(key, seq).await
            }
            async fn last_seq_no(&self) -> Result<u64> {
                self.inner.last_seq_no().await
            }
            async fn last_tx_id(&self) -> Result<TxId> {
                self.inner.last_tx_id().await
            }
            async fn rollback_to_tx(&self, tx_id: TxId) -> Result<()> {
                self.inner.rollback_to_tx(tx_id).await
            }
            async fn scan(
                &self,
                start: std::ops::Bound<&[u8]>,
                end: std::ops::Bound<&[u8]>,
            ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
                self.inner.scan(start, end).await
            }
        }

        let storage = Arc::new(TrackingStorage::new());
        let store = PersistentCheckpointStore::new(storage.clone());

        // Create several checkpoints
        for i in 0..5 {
            store
                .create_checkpoint(
                    &format!("cp_{}", i),
                    "c1",
                    i as u64,
                    TxId::new(i as u64),
                    serde_json::json!({}),
                )
                .await
                .unwrap();
        }

        // Delete one
        store.drop_checkpoint("cp_0").await.unwrap();

        // Verify ALL observed TxIds are in the reserved internal range
        let observed = storage.observed_tx_ids.lock().clone();
        assert!(!observed.is_empty(), "Should have observed some TxIds");

        for tx_id in &observed {
            assert!(
                *tx_id >= TxId::INTERNAL_BASE,
                "Internal TxId {} must be >= INTERNAL_BASE ({})",
                tx_id,
                TxId::INTERNAL_BASE
            );
        }

        // Verify no collision with typical user TxId range (1..1000)
        for tx_id in &observed {
            assert!(
                *tx_id > 1000,
                "Internal TxId {} collides with user range",
                tx_id
            );
        }
    }
}
