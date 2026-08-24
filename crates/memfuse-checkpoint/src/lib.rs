//! Checkpoint-Registry für Time-Travel und MVCC-basiertes Snapshotting.
//!
//! # Architektur
//! `PersistentCheckpointStore` delegiert Persistenz an ein [`memfuse_core::StorageEngine`]-Objekt
//! und cacht aktive Checkpoints in einem thread-sicheren In-Memory-Store (`parking_lot::RwLock`).

#![forbid(unsafe_code)]

use async_trait::async_trait;
use memfuse_core::{MemFuseError, Result, TxId, WorkflowState};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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

/// Point-in-Time Checkpoint representing an agent step or transaction boundary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StateCheckpoint {
    pub tx_id: TxId,
    pub timestamp_ms: u64,
}

/// RAII Guard that rolls back a checkpoint if not explicitly committed.
/// Prevents transaction leaks if the process panics or drops early.
pub struct CheckpointGuard<S: memfuse_core::StorageEngine> {
    checkpoint: Option<StateCheckpoint>,
    storage: Arc<S>,
}

impl<S: memfuse_core::StorageEngine> CheckpointGuard<S> {
    pub fn new(checkpoint: StateCheckpoint, storage: Arc<S>) -> Self {
        Self {
            checkpoint: Some(checkpoint),
            storage,
        }
    }

    pub fn checkpoint(&self) -> Result<&StateCheckpoint> {
        self.checkpoint
            .as_ref()
            .ok_or_else(|| MemFuseError::Internal("Checkpoint already consumed".into()))
    }

    pub fn commit(mut self) -> Result<StateCheckpoint> {
        self.checkpoint
            .take()
            .ok_or_else(|| MemFuseError::Internal("Checkpoint already consumed".into()))
    }
}

impl<S: memfuse_core::StorageEngine> Drop for CheckpointGuard<S> {
    fn drop(&mut self) {
        if let Some(cp) = self.checkpoint.take() {
            tracing::warn!(
                "CheckpointGuard dropped without commit. Auto-rolling back to TxId: {:?}",
                cp.tx_id
            );
            let storage_clone = Arc::clone(&self.storage);
            tokio::spawn(async move {
                if let Err(e) = storage_clone.rollback_to_tx(cp.tx_id).await {
                    tracing::error!("Auto-rollback failed: {}", e);
                }
            });
        }
    }
}

/// Trait für die Checkpoint-Verwaltung.
#[async_trait]
pub trait CheckpointRegistry: Send + Sync {
    async fn save_checkpoint(&self, meta: CheckpointMeta) -> Result<()>;
    async fn load_checkpoint(&self, seq_no: u64) -> Result<Option<CheckpointMeta>>;
    async fn list_checkpoints(&self) -> Result<Vec<CheckpointMeta>>;
}

/// Registry für gespeicherte Checkpoints mit Thread-sicherem Zustand.
///
/// # Invarianten
/// - Alle Methoden sind durch `RwLock` thread-sicher
/// - `StorageEngine`-Zugriffe nutzen atomare Transaktionen via `TxId`
/// - Keine Panics (Zero-Panic Doctrine)
pub struct PersistentCheckpointStore<S: memfuse_core::StorageEngine> {
    storage: Arc<S>,
    /// Registrierte Checkpoints im Arbeitsspeicher — geschützt durch RwLock (seq_no -> meta)
    checkpoints: RwLock<HashMap<u64, CheckpointMeta>>,
    /// O(1) Index für Name -> seq_no Lookup
    name_index: RwLock<HashMap<String, u64>>,
    /// Namespace-Präfix für Storage-Keys
    namespace: String,
    /// Lock für sequentielle Schreiboperationen auf den Storage (HIGH-002)
    write_lock: tokio::sync::Mutex<()>,
    /// Atomarer Zähler für interne TxIds (vermeidet Kollisionen)
    next_internal_tx: std::sync::atomic::AtomicU64,
}

impl<S: memfuse_core::StorageEngine> PersistentCheckpointStore<S> {
    pub fn new(storage: Arc<S>, namespace: impl Into<String>) -> Self {
        Self {
            storage,
            checkpoints: RwLock::new(HashMap::new()),
            name_index: RwLock::new(HashMap::new()),
            namespace: namespace.into(),
            write_lock: tokio::sync::Mutex::new(()),
            next_internal_tx: std::sync::atomic::AtomicU64::new(0),
        }
    }

    fn next_tx(&self) -> TxId {
        TxId::new(
            TxId::INTERNAL_BASE
                + self
                    .next_internal_tx
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst),
        )
    }

    /// Creates an ephemeral transactional checkpoint RAII guard.
    /// If the returned guard is dropped without calling `.commit()`, the underlying storage
    /// is automatically rolled back to `tx_id`.
    pub fn create_guard(&self, tx_id: TxId) -> Result<CheckpointGuard<S>> {
        let timestamp_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| MemFuseError::Storage(format!("System clock error: {}", e)))?
            .as_millis() as u64;
        let cp = StateCheckpoint {
            tx_id,
            timestamp_ms,
        };
        Ok(CheckpointGuard::new(cp, Arc::clone(&self.storage)))
    }

    /// Creates a new persistent checkpoint.
    pub async fn create_checkpoint(
        &self,
        name: &str,
        collection_id: &str,
        seq_no: u64,
        tx_id: TxId,
        metadata: serde_json::Value,
    ) -> Result<CheckpointMeta> {
        let meta = CheckpointMeta {
            name: name.to_string(),
            collection_id: collection_id.to_string(),
            seq_no,
            tx_id,
            metadata,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| MemFuseError::Internal(e.to_string()))?
                .as_secs(),
        };

        let _guard = self.write_lock.lock().await;

        // Lade alten Checkpoint (für späteres Unpin)
        let old_checkpoint = self.get_checkpoint_internal(name).await?;

        // 1. NEU: Pin des NEUEN Checkpoints ZUERST
        self.storage.pin_checkpoint(seq_no).await?;

        // 2. Persistiere den neuen Checkpoint
        let save_result = self.save_checkpoint_internal(meta.clone()).await;

        if let Err(e) = save_result {
            // Rollback: neuen Checkpoint entpinnen (save fehlgeschlagen)
            let _ = self.storage.unpin_checkpoint(seq_no).await;
            return Err(e);
        }

        // 3. ERST JETZT alten Checkpoint entpinnen (sicher, weil neuer gespeichert)
        if let Some(old) = old_checkpoint {
            if old.seq_no != seq_no {
                if let Err(e) = self.storage.unpin_checkpoint(old.seq_no).await {
                    // Nicht-fatal: loggern aber fortfahren
                    tracing::warn!(
                        old_seq = old.seq_no,
                        "Konnte alten Checkpoint nicht entpinnen: {e}"
                    );
                }
                // Alten Checkpoint aus Cache entfernen
                self.checkpoints.write().remove(&old.seq_no);
                self.name_index.write().remove(&old.name);
            }
        }

        // 4. Neuen Checkpoint in Cache eintragen
        self.checkpoints.write().insert(seq_no, meta.clone());
        self.name_index.write().insert(name.to_string(), seq_no);

        Ok(meta)
    }

    /// Deletes a persistent checkpoint by name.
    pub async fn drop_checkpoint(&self, name: &str) -> Result<()> {
        let _guard = self.write_lock.lock().await;

        if let Some(checkpoint) = self.get_checkpoint_internal(name).await? {
            // 1. Zuerst aus Storage löschen (mit eindeutiger TxId)
            let key = format!("{}:checkpoint:{}", self.namespace, name);

            // FIX CHK-002: Generiere eine eindeutige TxId statt INTERNAL_BASE
            let unique_tx = self.next_tx();

            if let Err(e) = self.storage.delete(unique_tx, key.as_bytes()).await {
                let _ = self.storage.rollback(unique_tx).await;
                return Err(e);
            }
            if let Err(e) = self.storage.commit(unique_tx).await {
                let _ = self.storage.rollback(unique_tx).await;
                return Err(e);
            }

            // 2. Erst nach erfolgreichem Storage-Delete entpinnen
            if let Err(e) = self.storage.unpin_checkpoint(checkpoint.seq_no).await {
                tracing::warn!(
                    seq = checkpoint.seq_no,
                    "Unpin nach drop fehlgeschlagen: {e}"
                );
            }

            // 3. Cache bereinigen
            self.checkpoints.write().remove(&checkpoint.seq_no);
            self.name_index.write().remove(&checkpoint.name);
        }
        Ok(())
    }

    /// Helper for internal saving logic. Uses name as key for uniqueness.
    async fn save_checkpoint_internal(&self, meta: CheckpointMeta) -> Result<()> {
        let key = format!("{}:checkpoint:{}", self.namespace, meta.name);
        let value =
            serde_json::to_vec(&meta).map_err(|e| MemFuseError::Serialization(e.to_string()))?;

        let tx = self.next_tx();
        if let Err(e) = self.storage.put(tx, key.as_bytes(), &value).await {
            let _ = self.storage.rollback(tx).await;
            return Err(e);
        }
        if let Err(e) = self.storage.commit(tx).await {
            let _ = self.storage.rollback(tx).await;
            return Err(e);
        }

        // In-Memory Cache aktualisieren
        self.name_index
            .write()
            .insert(meta.name.clone(), meta.seq_no);
        self.checkpoints.write().insert(meta.seq_no, meta);
        Ok(())
    }

    /// Internal helper to get checkpoint by name without extra locking.
    async fn get_checkpoint_internal(&self, name: &str) -> Result<Option<CheckpointMeta>> {
        // Erst O(1) In-Memory Name-Index prüfen
        {
            let name_idx = self.name_index.read();
            if let Some(&seq_no) = name_idx.get(name) {
                let cache = self.checkpoints.read();
                if let Some(cp) = cache.get(&seq_no) {
                    return Ok(Some(cp.clone()));
                }
            }
        }

        // Storage direkt fragen
        let key = format!("{}:checkpoint:{}", self.namespace, name);
        match self.storage.get(key.as_bytes()).await? {
            Some(bytes) => {
                let meta: CheckpointMeta = serde_json::from_slice(&bytes)
                    .map_err(|e| MemFuseError::Serialization(e.to_string()))?;
                self.name_index
                    .write()
                    .insert(meta.name.clone(), meta.seq_no);
                self.checkpoints.write().insert(meta.seq_no, meta.clone());
                Ok(Some(meta))
            }
            None => Ok(None),
        }
    }

    /// Public inherent methods for compatibility
    pub async fn list_checkpoints(&self) -> Result<Vec<CheckpointMeta>> {
        let prefix = format!("{}:checkpoint:", self.namespace);
        let entries: Vec<(Vec<u8>, Vec<u8>)> = self.storage.scan_prefix(prefix.as_bytes()).await?;

        let mut result = Vec::with_capacity(entries.len());
        for (_key_bytes, value_bytes) in entries {
            let meta: CheckpointMeta = serde_json::from_slice(&value_bytes)
                .map_err(|e| MemFuseError::Serialization(e.to_string()))?;
            result.push(meta);
        }

        // Cache synchronisieren
        {
            let mut cache = self.checkpoints.write();
            let mut name_idx = self.name_index.write();
            cache.clear();
            name_idx.clear();
            for meta in &result {
                cache.insert(meta.seq_no, meta.clone());
                name_idx.insert(meta.name.clone(), meta.seq_no);
            }
        }

        result.sort_by_key(|m| m.seq_no);
        Ok(result)
    }

    pub async fn get_checkpoint(&self, name: &str) -> Result<Option<CheckpointMeta>> {
        self.get_checkpoint_internal(name).await
    }

    /// Restores the system to a specific checkpoint by name.
    /// This will rollback the underlying storage to the transaction ID of the checkpoint.
    pub async fn restore_checkpoint(&self, name: &str) -> Result<CheckpointMeta> {
        let meta = self
            .get_checkpoint_internal(name)
            .await?
            .ok_or_else(|| MemFuseError::NotFound(format!("Checkpoint '{}' not found", name)))?;

        let _guard = self.write_lock.lock().await;

        // 1. Rollback storage state
        self.storage.rollback_to_tx(meta.tx_id).await?;

        // 2. Synchronize cache
        let _ = self.list_checkpoints().await?;

        Ok(meta)
    }
}

#[async_trait]
impl<S: memfuse_core::StorageEngine> CheckpointRegistry for PersistentCheckpointStore<S> {
    async fn save_checkpoint(&self, meta: CheckpointMeta) -> Result<()> {
        let _guard = self.write_lock.lock().await;
        self.save_checkpoint_internal(meta).await
    }

    async fn load_checkpoint(&self, seq_no: u64) -> Result<Option<CheckpointMeta>> {
        // Erst In-Memory prüfen
        if let Some(meta) = self.checkpoints.read().get(&seq_no) {
            return Ok(Some(meta.clone()));
        }

        // Dann Storage via Scan (da Key auf Name basiert)
        let all = self.list_checkpoints().await?;
        Ok(all.into_iter().find(|c| c.seq_no == seq_no))
    }

    async fn list_checkpoints(&self) -> Result<Vec<CheckpointMeta>> {
        self.list_checkpoints().await
    }
}

#[async_trait]
impl<S: memfuse_core::StorageEngine> memfuse_core::traits::CheckpointCoordinator
    for PersistentCheckpointStore<S>
{
    type Meta = CheckpointMeta;

    async fn create_named_checkpoint(
        &self,
        name: &str,
        collection_id: &str,
        seq_no: u64,
        tx_id: TxId,
        metadata: serde_json::Value,
    ) -> Result<Self::Meta> {
        self.create_checkpoint(name, collection_id, seq_no, tx_id, metadata)
            .await
    }

    async fn restore_named_checkpoint(&self, name: &str) -> Result<Self::Meta> {
        self.restore_checkpoint(name).await
    }

    async fn drop_named_checkpoint(&self, name: &str) -> Result<()> {
        self.drop_checkpoint(name).await
    }

    async fn list_named_checkpoints(&self) -> Result<Vec<Self::Meta>> {
        self.list_checkpoints().await
    }
}

#[async_trait]
impl<S: memfuse_core::StorageEngine> memfuse_core::traits::Checkpoint
    for PersistentCheckpointStore<S>
{
    async fn take_snapshot(&self, tx: TxId) -> Result<WorkflowState> {
        let seq_no = self.storage.last_seq_no().await?;
        Ok(WorkflowState {
            tx,
            graph_hash: format!("seq-{}", seq_no),
        })
    }

    async fn restore(&self, state: &WorkflowState) -> Result<()> {
        self.storage.rollback_to_tx(state.tx).await?;
        let _ = self.list_checkpoints().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use memfuse_core::{Result, StorageEngine, StorageStats};
    use parking_lot::Mutex;
    use std::collections::HashSet;

    struct MockStorage {
        data: Mutex<HashMap<Vec<u8>, Vec<u8>>>,
        pinned: Mutex<HashSet<u64>>,
        fail_on_put: Mutex<Option<Vec<u8>>>,
    }

    impl MockStorage {
        fn new() -> Self {
            Self {
                data: Mutex::new(HashMap::new()),
                pinned: Mutex::new(HashSet::new()),
                fail_on_put: Mutex::new(None),
            }
        }
    }

    #[async_trait]
    impl StorageEngine for MockStorage {
        async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
            Ok(self.data.lock().get(key).cloned())
        }
        async fn get_at_seq(&self, key: &[u8], _seq: u64) -> Result<Option<Vec<u8>>> {
            self.get(key).await
        }
        async fn put(&self, _tx_id: TxId, key: &[u8], value: &[u8]) -> Result<()> {
            if let Some(fail_key) = self.fail_on_put.lock().as_ref() {
                if key == fail_key {
                    return Err(MemFuseError::Internal("Mock Storage Error".to_string()));
                }
            }
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

    #[tokio::test]
    async fn test_create_and_load() {
        let storage = Arc::new(MockStorage::new());
        let store = PersistentCheckpointStore::new(storage, "test");
        let meta = store
            .create_checkpoint("cp1", "c1", 1, TxId::new(1), serde_json::json!({}))
            .await
            .unwrap();
        let loaded = store.load_checkpoint(1).await.unwrap().unwrap();
        assert_eq!(loaded, meta);
    }

    #[tokio::test]
    async fn test_name_uniqueness() {
        let storage = Arc::new(MockStorage::new());
        let store = PersistentCheckpointStore::new(storage.clone(), "test");
        store
            .create_checkpoint("same", "c1", 1, TxId::new(1), serde_json::json!({}))
            .await
            .unwrap();
        store
            .create_checkpoint("same", "c1", 2, TxId::new(2), serde_json::json!({}))
            .await
            .unwrap();
        let all = store.list_checkpoints().await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].seq_no, 2);
        assert!(!storage.pinned.lock().contains(&1));
        assert!(storage.pinned.lock().contains(&2));
    }

    #[tokio::test]
    async fn test_checkpoint_creation_rollback_on_failure() {
        let storage = Arc::new(MockStorage::new());
        let cp_key = b"test:checkpoint:fail_cp";
        *storage.fail_on_put.lock() = Some(cp_key.to_vec());

        let store = PersistentCheckpointStore::new(storage.clone(), "test");
        let seq_no = 123;

        let res = store
            .create_checkpoint("fail_cp", "c1", seq_no, TxId::new(1), serde_json::json!({}))
            .await;

        assert!(res.is_err());
        assert!(!storage.pinned.lock().contains(&seq_no));
    }

    #[tokio::test]
    async fn test_pin_before_unpin_invariant_on_failure() {
        let storage = Arc::new(MockStorage::new());
        let store = PersistentCheckpointStore::new(storage.clone(), "test");

        // 1. Create first checkpoint successfully
        store
            .create_checkpoint("my_cp", "c1", 1, TxId::new(1), serde_json::json!({}))
            .await
            .unwrap();

        assert!(storage.pinned.lock().contains(&1));

        // 2. Make next save fail
        let cp_key = b"test:checkpoint:my_cp";
        *storage.fail_on_put.lock() = Some(cp_key.to_vec());

        // 3. Try to overwrite with a new checkpoint, which will fail
        let res = store
            .create_checkpoint("my_cp", "c1", 2, TxId::new(2), serde_json::json!({}))
            .await;

        assert!(res.is_err());

        // 4. Verify invariant: old checkpoint (1) must still be pinned!
        assert!(
            storage.pinned.lock().contains(&1),
            "Old checkpoint should still be pinned because save failed"
        );

        // 5. Verify invariant: new checkpoint (2) should be unpinned (rolled back)!
        assert!(
            !storage.pinned.lock().contains(&2),
            "New checkpoint should be unpinned after failure"
        );
    }

    #[tokio::test]
    async fn test_checkpoint_guard_auto_rollback() {
        let storage = Arc::new(MockStorage::new());
        let store = PersistentCheckpointStore::new(storage.clone(), "test");

        {
            let _guard = store.create_guard(TxId::new(42)).unwrap();
            // guard drops here without commit
        }

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        // Verify no panic and guard auto-rollback dropped cleanly
    }

    #[tokio::test]
    async fn test_checkpoint_guard_commit() {
        let storage = Arc::new(MockStorage::new());
        let store = PersistentCheckpointStore::new(storage.clone(), "test");

        let guard = store.create_guard(TxId::new(100)).unwrap();
        let cp = guard.commit().unwrap();
        assert_eq!(cp.tx_id, TxId::new(100));
    }
}
