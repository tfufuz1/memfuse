//! Checkpoint management for MemFuse.
//! Provides point-in-time snapshots of the database state.

#![forbid(unsafe_code)]

use async_trait::async_trait;
use memfuse_core::{MemFuseError, Result, StorageEngine, TxId};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Checkpoint {
    pub name: String,
    pub collection_id: String,
    pub seq_no: u64,
    pub metadata: serde_json::Value,
}

/// Registry for active transaction intent checkpoints.
pub struct CheckpointRegistry {
    checkpoints: Arc<Mutex<std::collections::HashMap<TxId, CheckpointIntent>>>,
}

#[derive(Debug, Clone)]
pub struct CheckpointIntent {
    pub collection_id: String,
    pub graph_hash: String,
}

impl CheckpointRegistry {
    pub fn new() -> Self {
        Self {
            checkpoints: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
    }

    pub async fn register(&self, tx_id: TxId, intent: CheckpointIntent) {
        let mut guard = self.checkpoints.lock().await;
        guard.insert(tx_id, intent);
    }

    pub async fn get(&self, tx_id: TxId) -> Option<CheckpointIntent> {
        let guard = self.checkpoints.lock().await;
        guard.get(&tx_id).cloned()
    }

    pub async fn remove(&self, tx_id: TxId) {
        let mut guard = self.checkpoints.lock().await;
        guard.remove(&tx_id);
    }
}

impl Default for CheckpointRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Manages database checkpoints.
pub struct CheckpointManager {
    storage: Arc<dyn StorageEngine>,
}

impl CheckpointManager {
    pub fn new(storage: Arc<dyn StorageEngine>) -> Self {
        Self { storage }
    }

    /// Creates a new checkpoint.
    pub async fn create_checkpoint(
        &self,
        name: &str,
        collection_id: &str,
        seq_no: u64,
        metadata: serde_json::Value,
    ) -> Result<Checkpoint> {
        let checkpoint = Checkpoint {
            name: name.to_string(),
            collection_id: collection_id.to_string(),
            seq_no,
            metadata,
        };

        let key = format!("__checkpoint:{}", name);
        let value = serde_json::to_vec(&checkpoint)
            .map_err(|e| MemFuseError::Storage(format!("Serialization failed: {}", e)))?;

        // Pin the sequence number in storage to prevent GC
        self.storage.pin_checkpoint(seq_no).await?;

        // Use a dummy TxId for internal metadata update
        let tx = TxId::new(u64::MAX);
        self.storage.put(tx, key.as_bytes(), &value).await?;
        self.storage.commit(tx).await?;

        Ok(checkpoint)
    }

    /// Retrieves a checkpoint by name.
    pub async fn get_checkpoint(&self, name: &str) -> Result<Option<Checkpoint>> {
        let key = format!("__checkpoint:{}", name);
        let data = self.storage.get(key.as_bytes()).await?;

        if let Some(bytes) = data {
            let checkpoint: Checkpoint = serde_json::from_slice(&bytes)
                .map_err(|e| MemFuseError::Storage(format!("Deserialization failed: {}", e)))?;
            Ok(Some(checkpoint))
        } else {
            Ok(None)
        }
    }

    /// Lists all available checkpoints.
    pub async fn list_checkpoints(&self) -> Result<Vec<Checkpoint>> {
        let prefix = b"__checkpoint:";
        let entries = self.storage.scan_prefix(prefix).await?;
        let mut checkpoints = Vec::new();

        for (_, value) in entries {
            let checkpoint: Checkpoint = serde_json::from_slice(&value)
                .map_err(|e| MemFuseError::Storage(format!("Deserialization failed: {}", e)))?;
            checkpoints.push(checkpoint);
        }

        Ok(checkpoints)
    }

    /// Deletes a checkpoint.
    pub async fn drop_checkpoint(&self, name: &str) -> Result<()> {
        let cp = self.get_checkpoint(name).await?;
        if let Some(checkpoint) = cp {
            let key = format!("__checkpoint:{}", name);
            let tx = TxId::new(u64::MAX);
            self.storage.delete(tx, key.as_bytes()).await?;
            self.storage.commit(tx).await?;

            // Unpin the version to allow GC
            self.storage.unpin_checkpoint(checkpoint.seq_no).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use parking_lot::Mutex as SyncMutex;

    struct MockStorage {
        data: SyncMutex<std::collections::HashMap<Vec<u8>, Vec<u8>>>,
        pinned: SyncMutex<HashSet<u64>>,
    }

    impl MockStorage {
        fn new() -> Self {
            Self {
                data: SyncMutex::new(std::collections::HashMap::new()),
                pinned: SyncMutex::new(HashSet::new()),
            }
        }
    }

    #[async_trait]
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
        async fn scan_prefix(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
            let guard = self.data.lock();
            let mut results = Vec::new();
            for (k, v) in guard.iter() {
                if k.starts_with(prefix) {
                    results.push((k.clone(), v.clone()));
                }
            }
            results.sort_by(|a, b| a.0.cmp(&b.0));
            Ok(results)
        }
        async fn commit(&self, _tx_id: TxId) -> Result<()> { Ok(()) }
        async fn rollback(&self, _tx_id: TxId) -> Result<()> { Ok(()) }
        async fn flush(&self) -> Result<()> { Ok(()) }
        async fn pin_checkpoint(&self, seq_no: u64) -> Result<()> {
            self.pinned.lock().insert(seq_no);
            Ok(())
        }
        async fn unpin_checkpoint(&self, seq_no: u64) -> Result<()> {
            self.pinned.lock().remove(&seq_no);
            Ok(())
        }
        async fn last_seq_no(&self) -> Result<u64> { Ok(0) }
        async fn stats(&self) -> Result<memfuse_core::StorageStats> {
            Ok(memfuse_core::StorageStats {
                num_segments: 0,
                total_size_bytes: 0,
                memtable_size_bytes: 0,
            })
        }
    }

    #[tokio::test]
    async fn test_checkpoint_lifecycle() {
        let storage = Arc::new(MockStorage::new());
        let manager = CheckpointManager::new(storage.clone());

        let meta = manager
            .create_checkpoint("test_cp", "coll_1", 100, serde_json::json!({"state": "ok"}))
            .await
            .unwrap(); // unwrap

        assert_eq!(meta.name, "test_cp");
        assert_eq!(meta.seq_no, 100);

        // Verify it was pinned
        assert!(storage.pinned.lock().contains(&100));

        // Verify it exists in manager
        let retrieved = manager.get_checkpoint("test_cp").await.unwrap().unwrap(); // unwrap
        assert_eq!(retrieved, meta);
    }

    #[tokio::test]
    async fn test_checkpoint_metadata_roundtrip() {
        let storage = Arc::new(MockStorage::new());
        let manager = CheckpointManager::new(storage.clone());
        let metadata = serde_json::json!({"version": 1, "tags": ["stable", "prod"]});

        manager
            .create_checkpoint("cp1", "c1", 500, metadata.clone())
            .await
            .unwrap(); // unwrap

        let retrieved = manager.get_checkpoint("cp1").await.unwrap().unwrap(); // unwrap
        assert_eq!(retrieved.metadata, metadata);
    }

    #[tokio::test]
    async fn test_list_checkpoints() {
        let storage = Arc::new(MockStorage::new());
        let manager = CheckpointManager::new(storage.clone());

        manager
            .create_checkpoint("cp1", "c1", 10, serde_json::json!({}))
            .await
            .unwrap(); // unwrap
        manager
            .create_checkpoint("cp2", "c1", 20, serde_json::json!({}))
            .await
            .unwrap(); // unwrap
        manager
            .create_checkpoint("cp3", "c1", 30, serde_json::json!({}))
            .await
            .unwrap(); // unwrap

        let list = manager.list_checkpoints().await.unwrap(); // unwrap
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].name, "cp1");
        assert_eq!(list[1].name, "cp2");
        assert_eq!(list[2].name, "cp3");
    }

    #[tokio::test]
    async fn test_drop_checkpoint() {
        let storage = Arc::new(MockStorage::new());
        let manager = CheckpointManager::new(storage.clone());

        manager
            .create_checkpoint("persist_me", "c1", 50, serde_json::json!({}))
            .await
            .unwrap(); // unwrap
        assert!(storage.pinned.lock().contains(&50));

        let list = manager.list_checkpoints().await.unwrap(); // unwrap
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "persist_me");
        assert_eq!(list[0].seq_no, 50);

        manager.drop_checkpoint("persist_me").await.unwrap(); // unwrap
        assert!(!storage.pinned.lock().contains(&50));
        assert_eq!(manager.list_checkpoints().await.unwrap().len(), 0); // unwrap
    }

    #[tokio::test]
    async fn test_checkpoint_registry() {
        let registry = CheckpointRegistry::new();
        let tx_id = TxId::new(101);
        let intent = CheckpointIntent {
            collection_id: "coll_a".to_string(),
            graph_hash: "hash".to_string(),
        };

        registry.register(tx_id, intent).await;
        let retrieved = registry.get(tx_id).await.unwrap(); // unwrap
        assert_eq!(retrieved.graph_hash, "hash");

        registry.remove(tx_id).await;
        assert!(registry.get(tx_id).await.is_none());
    }
}
