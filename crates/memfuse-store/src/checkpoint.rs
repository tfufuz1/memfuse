// FILE-CONTEXT
// STAND: 2026-08-30T21:49:55Z (SESSION: 283abf0f)
// ZWECK: Crate-internes MVCC Snapshot-Pinning und TxId-skopierte Rollbacks.
// INVARIANTEN: Crate-intern (pub(crate)) — öffentliche Checkpoints nur via memfuse-checkpoint (ADR-011).
// HOTSPOTS: Checkpointer::rollback_to
// SIEHE AUCH: DECISIONS.md ADR-011, ADR-015

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
use memfuse_core::{Result, TxId};
use std::sync::Arc;

/// Represents a Point-in-Time snapshot of the agent's memory state.
#[derive(Debug, Clone)]
pub struct StateCheckpoint {
    pub tx_id: TxId,
    pub timestamp_ms: u64,
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

        let cp1 = StateCheckpoint {
            tx_id: tx1,
            timestamp_ms: 0,
        };

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
    }
}
