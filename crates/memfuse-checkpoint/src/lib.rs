//! Checkpointing & Time-Travel (WP-5.1)
//!
//! Enables deterministic freezing and restarting of agent workflows.
//! Implements a SnapshotRegistry abstracting over Multi-Version Concurrency Control (MVCC).

#![forbid(unsafe_code)]

use memfuse_core::{Result, StorageEngine, StorageStats, TxId, WorkflowState};
use parking_lot::RwLock as SyncRwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Metadata for a persistent checkpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointMeta {
    pub name: String,
    pub collection_id: String,
    pub seq_no: u64,
    pub metadata: serde_json::Value,
    pub created_at: u64,
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
pub struct PersistentCheckpointStore {
    storage: Arc<dyn StorageEngine>,
    // In-memory cache of checkpoints for fast lookups
    persistent_checkpoints: SyncRwLock<Vec<CheckpointMeta>>,
    registry: Arc<CheckpointRegistry>,
    // Serializes write operations to ensure atomicity between storage and cache
    op_lock: tokio::sync::Mutex<()>,
}

impl PersistentCheckpointStore {
    pub fn new(storage: Arc<dyn StorageEngine>) -> Self {
        Self {
            storage,
            persistent_checkpoints: SyncRwLock::new(Vec::new()),
            registry: Arc::new(CheckpointRegistry::new()),
            op_lock: tokio::sync::Mutex::new(()),
        }
    }

    /// Creates a new persistent checkpoint at the specified sequence number.
    pub async fn create_checkpoint(
        &self,
        name: &str,
        collection_id: &str,
        seq_no: u64,
        metadata: serde_json::Value,
    ) -> Result<CheckpointMeta> {
        let _guard = self.op_lock.lock().await;

        let checkpoint = CheckpointMeta {
            name: name.to_string(),
            collection_id: collection_id.to_string(),
            seq_no,
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

        // We use a dummy TxId for now or zero since it's an internal write
        self.storage
            .put(TxId::new(0), key.as_bytes(), &value)
            .await?;
        self.storage.commit(TxId::new(0)).await?;

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
    pub async fn reload_from_storage(&self) -> Result<()> {
        let _guard = self.op_lock.lock().await;

        let entries = self.storage.scan_prefix(b"__checkpoint:").await?;
        let mut checkpoints = Vec::new();
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
            self.storage.delete(TxId::new(0), key.as_bytes()).await?;
            self.storage.commit(TxId::new(0)).await?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use memfuse_core::Result;
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
            .create_checkpoint("test_cp", "coll_1", 100, serde_json::json!({"state": "ok"}))
            .await
            .unwrap();

        assert_eq!(meta.name, "test_cp");
        assert_eq!(meta.seq_no, 100);

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
            .create_checkpoint("cp1", "c1", 10, metadata.clone())
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
            .create_checkpoint("cp2", "c1", 20, serde_json::json!({}))
            .await
            .unwrap();
        manager
            .create_checkpoint("cp1", "c1", 10, serde_json::json!({}))
            .await
            .unwrap();
        manager
            .create_checkpoint("cp3", "c1", 30, serde_json::json!({}))
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
            .create_checkpoint("persist_me", "c1", 50, serde_json::json!({}))
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
                    .create_checkpoint(&name, "c1", i as u64, serde_json::json!({}))
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
}
