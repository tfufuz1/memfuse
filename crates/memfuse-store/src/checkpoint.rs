//! Native State Checkpointing (Crate-internal MVCC Snapshot-Pinning).
//!
//! # Architektur & Sichtbarkeit
//! Dieser Modul ist strikt crate-intern (`pub(crate)`). Er bietet LSM-spezifisches Snapshot-Pinning
//! und TxId-skopierte Transactional Rollbacks für MVCC.
//!
//! WARNUNG: Dieser Typ darf NIEMALS außerhalb von `memfuse-store` exportiert oder direkt verwendet werden.
//! Für die öffentliche Checkpoint-API (benannte Checkpoints, Trait-basierter `CheckpointCoordinator`, RAII `CheckpointGuard`)
//! ist gemäß ADR-011 ausschließlich `memfuse-checkpoint` zu verwenden.

#![allow(dead_code)]

// DECISION-REF: ADR-011 — Consolidated Checkpoint Subsystem Architecture
// DECISION-REF: ADR-015 — Integration von RAII CheckpointGuard in memfuse-checkpoint (AGT-CKPT-001 / AGT-STORE-002)
// ARCHITEKTUR: `memfuse-checkpoint` stellt den generischen `CheckpointGuard<S: StorageEngine>` und `PersistentCheckpointStore`
//             bereit. `memfuse-store::checkpoint` bietet LSM-spezifische transactional rollbacks (TxId-skopiert).
use crate::lsm::LsmStorage;
use memfuse_core::{MemFuseError, Result, TxId};
use std::sync::Arc;

/// Represents a Point-in-Time snapshot of the agent's memory state.
#[derive(Debug, Clone)]
pub struct StateCheckpoint {
    pub tx_id: TxId,
    pub timestamp_ms: u64,
}

/// RAII Guard that rolls back a checkpoint if not explicitly committed.
/// Prevents transaction leaks if the process panics or drops early.
pub struct CheckpointGuard {
    checkpoint: Option<StateCheckpoint>,
    storage: Arc<LsmStorage>,
}

impl CheckpointGuard {
    pub const fn new(checkpoint: StateCheckpoint, storage: Arc<LsmStorage>) -> Self {
        Self {
            checkpoint: Some(checkpoint),
            storage,
        }
    }

    pub(crate) fn checkpoint(&self) -> Result<&StateCheckpoint> {
        self.checkpoint
            .as_ref()
            .ok_or_else(|| MemFuseError::Internal("Checkpoint already consumed".into()))
    }

    pub(crate) fn commit(mut self) -> Result<StateCheckpoint> {
        self.checkpoint
            .take()
            .ok_or_else(|| MemFuseError::Internal("Checkpoint already consumed".into()))
    }
}

impl Drop for CheckpointGuard {
    fn drop(&mut self) {
        if let Some(cp) = self.checkpoint.take() {
            tracing::warn!(
                "CheckpointGuard dropped without commit. Auto-rolling back to TxId: {:?}",
                cp.tx_id
            );
            let storage_clone = Arc::clone(&self.storage);
            self.storage.spawn_tracked(async move {
                if let Err(e) = storage_clone.rollback_to_tx(cp.tx_id).await {
                    tracing::error!("Auto-rollback failed: {}", e);
                }
            });
        }
    }
}

/// The Checkpointer manages WAL replay bounds for deterministic time-travel.
pub struct Checkpointer {
    storage: Arc<LsmStorage>,
}

impl Checkpointer {
    /// Creates a new Checkpointer.
    pub const fn new(storage: Arc<LsmStorage>) -> Self {
        Self { storage }
    }

    /// Records a new checkpoint at the current transaction ID marking an agent step.
    /// Returns a RAII guard that will rollback the state if dropped without commit.
    // DECISION-REF: AGT-STORE-001 resolved — SystemTime error propagated via Result instead of unwrap_or_default()
    pub fn create_checkpoint(&self, tx_id: TxId) -> Result<CheckpointGuard> {
        let timestamp_ms = u64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| MemFuseError::Storage(format!("System clock error: {}", e)))?
                .as_millis(),
        )
        .map_err(|e| MemFuseError::Storage(format!("Timestamp overflow: {}", e)))?;
        let cp = StateCheckpoint {
            tx_id,
            timestamp_ms,
        };
        Ok(CheckpointGuard::new(cp, Arc::clone(&self.storage)))
    }

    /// Rolls the database state back to a specific checkpoint.
    /// This is the foundation for Time-Travel Debugging in SAOS.
    pub(crate) async fn rollback_to(&self, checkpoint: &StateCheckpoint) -> Result<()> {
        tracing::info!(
            "Initiating Time-Travel Rollback to TX: {}",
            checkpoint.tx_id
        );
        self.storage.rollback_to_tx(checkpoint.tx_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsm::{LsmConfig, LsmStorage};
    use memfuse_core::StorageEngine;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_rollback_to_checkpoint() {
        let tmp = TempDir::new().expect("temp dir"); // expect
        let config = LsmConfig {
            path: tmp.path().to_path_buf(),
            ..Default::default()
        };
        let storage = Arc::new(LsmStorage::new(config).await.expect("create storage")); // expect
        let checkpointer = Checkpointer::new(storage.clone());

        let tx1 = TxId::new(1);
        storage.put(tx1, b"key1", b"val1").await.unwrap(); // unwrap
        storage.commit(tx1).await.unwrap(); // unwrap

        let cp1_guard = checkpointer.create_checkpoint(tx1).unwrap(); // unwrap
        let cp1 = cp1_guard.commit().unwrap(); // expect

        let tx2 = TxId::new(2);
        storage.put(tx2, b"key2", b"val2").await.unwrap(); // unwrap
        storage.commit(tx2).await.unwrap(); // unwrap

        assert_eq!(storage.get(b"key1").await.unwrap(), Some(b"val1".to_vec())); // unwrap
        assert_eq!(storage.get(b"key2").await.unwrap(), Some(b"val2".to_vec())); // unwrap

        checkpointer.rollback_to(&cp1).await.expect("rollback"); // expect

        assert_eq!(storage.get(b"key1").await.unwrap(), Some(b"val1".to_vec())); // unwrap
        assert_eq!(storage.get(b"key2").await.unwrap(), None); // unwrap

        let tx3 = TxId::new(3);
        storage.put(tx3, b"key3", b"val3").await.unwrap(); // unwrap
        storage.commit(tx3).await.unwrap(); // unwrap
        assert_eq!(storage.get(b"key3").await.unwrap(), Some(b"val3".to_vec()));
        // unwrap
        // unwrap
    }

    #[tokio::test]
    async fn test_checkpoint_raii_rollback() {
        let tmp = TempDir::new().expect("temp dir"); // expect
        let config = LsmConfig {
            path: tmp.path().to_path_buf(),
            ..Default::default()
        };
        let storage = Arc::new(LsmStorage::new(config).await.expect("create storage")); // expect
        let checkpointer = Checkpointer::new(storage.clone());

        let tx1 = TxId::new(1);
        storage.put(tx1, b"key1", b"val1").await.unwrap(); // unwrap
        storage.commit(tx1).await.unwrap(); // unwrap

        {
            // Create a checkpoint guard but do NOT commit it.
            let _cp_guard = checkpointer.create_checkpoint(tx1).unwrap(); // unwrap

            let tx2 = TxId::new(2);
            storage.put(tx2, b"key2", b"val2").await.unwrap(); // unwrap
            storage.commit(tx2).await.unwrap(); // unwrap
            assert_eq!(storage.get(b"key2").await.unwrap(), Some(b"val2".to_vec()));
            // unwrap
            // unwrap
            // guard drops here, triggering auto-rollback
        }

        // Wait a small amount of time for the spawned tokio task to finish
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Verify that tx2 was rolled back
        assert_eq!(storage.get(b"key1").await.unwrap(), Some(b"val1".to_vec())); // unwrap
        assert_eq!(storage.get(b"key2").await.unwrap(), None); // unwrap
    }
}
