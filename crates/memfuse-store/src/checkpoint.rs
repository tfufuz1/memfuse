//! Native State Checkpointing (Time-Travel Debugging).
//!
//! AUDIT:2026-05-23 STATUS:IMPLEMENTED (P0 Remediation)
//! Enables exact state reconstruction of an SAOS database at any given transaction ID.

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
    pub fn new(storage: Arc<LsmStorage>) -> Self {
        Self { storage }
    }

    /// Records a new checkpoint at the current transaction ID marking an agent step.
    pub fn create_checkpoint(&self, tx_id: TxId) -> StateCheckpoint {
        StateCheckpoint {
            tx_id,
            timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        }
    }

    /// Rolls the database state back to a specific checkpoint.
    /// This is the foundation for Time-Travel Debugging in SAOS.
    pub async fn rollback_to(&self, checkpoint: &StateCheckpoint) -> Result<()> {
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
        let tmp = TempDir::new().expect("temp dir");
        let config = LsmConfig {
            path: tmp.path().to_path_buf(),
            ..Default::default()
        };
        let storage = Arc::new(LsmStorage::new(config).await.expect("create storage"));
        let checkpointer = Checkpointer::new(storage.clone());

        // 1. Insert some data
        let tx1 = TxId::new(1);
        storage.put(tx1, b"key1", b"val1").await.unwrap(); // unwrap
        storage.commit(tx1).await.unwrap(); // unwrap

        let cp1 = checkpointer.create_checkpoint(tx1);

        // 2. Insert more data
        let tx2 = TxId::new(2);
        storage.put(tx2, b"key2", b"val2").await.unwrap(); // unwrap
        storage.commit(tx2).await.unwrap(); // unwrap

        assert_eq!(storage.get(b"key1").await.unwrap(), Some(b"val1".to_vec())); // unwrap
        assert_eq!(storage.get(b"key2").await.unwrap(), Some(b"val2".to_vec())); // unwrap

        // 3. Rollback to cp1
        checkpointer.rollback_to(&cp1).await.expect("rollback");

        // 4. Verify state
        assert_eq!(storage.get(b"key1").await.unwrap(), Some(b"val1".to_vec())); // unwrap
        assert_eq!(storage.get(b"key2").await.unwrap(), None); // Should be gone! // unwrap

        // 5. Verify we can still write and seq_no is correct
        let tx3 = TxId::new(3);
        storage.put(tx3, b"key3", b"val3").await.unwrap(); // unwrap
        storage.commit(tx3).await.unwrap(); // unwrap
        assert_eq!(storage.get(b"key3").await.unwrap(), Some(b"val3".to_vec()));
        // unwrap
    }
}
