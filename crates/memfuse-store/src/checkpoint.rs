//! Native State Checkpointing (Time-Travel Debugging).
//!
//! Enables exact state reconstruction of an SAOS database at any given transaction ID
//! by replaying the Write-Ahead Log (WAL) up to that point.

// ANCHOR:SPEC:WP5.1-CHECKPOINT-001 — Time-Travel Debugging ist aktuell ein STUB.
// WP:WP-5.1 PRIO:3 NEEDS:NONE
// AGENT:07 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-09 DEADLINE:NONE
// ANCHOR:FIXME:WP-5.1-ROLLBACK-STUB STATUS:REVIEW AGENT:02
// Funktionaler Rollback implementiert.
// PLAN: WAL bis checkpoint.tx_id replayed → deterministischer State-Restore.
// SPEC: docs/specs/SPEC-20260505-WP-4.x-Scale.md (State Checkpointing Sektion)

use crate::lsm::LsmStorage;
use memfuse_core::{Result, TxId};
use std::sync::Arc;
use std::time::SystemTime;

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
        let timestamp_ms = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        StateCheckpoint {
            tx_id,
            timestamp_ms,
        }
    }

    /// Rolls the database state back to a specific checkpoint.
    /// This is the foundation for Time-Travel Debugging in SAOS.
    pub async fn rollback_to(&self, checkpoint: &StateCheckpoint) -> Result<()> {
        tracing::info!(
            "Initiating Time-Travel Rollback to TX: {}",
            checkpoint.tx_id
        );
        // Process:
        // 1. Halt all active writes globally (inside rollback_to_tx).
        // 2. Drop current volatile MemTables and indices (inside rollback_to_tx).
        // 3. Replay WAL strictly up to `checkpoint.tx_id` (inside rollback_to_tx).
        // 4. Resume operations deterministically.
        self.storage.rollback_to_tx(checkpoint.tx_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsm::{LsmConfig, LsmStorage};
    use memfuse_core::StorageEngine;
    use tempfile::TempDir;
    use std::time::Duration;

    #[tokio::test]
    async fn test_checkpoint_rollback() {
        let tmp = TempDir::new().expect("temp dir");
        let config = LsmConfig {
            path: tmp.path().to_path_buf(),
            memtable_size_limit: 1024 * 1024,
            max_ram_mb: 64,
            tx_timeout: Duration::from_secs(60),
            compaction: Default::default(),
            encryption_passphrase: None,
        };
        let storage = Arc::new(LsmStorage::new(config).await.expect("create storage"));
        let checkpointer = Checkpointer::new(Arc::clone(&storage));

        // 1. Write some data
        let tx1 = TxId::new(1);
        storage.put(tx1, b"key1", b"val1").await.unwrap();
        storage.commit(tx1).await.unwrap();

        // 2. Create checkpoint
        let cp = checkpointer.create_checkpoint(tx1);

        // 3. Write more data
        let tx2 = TxId::new(2);
        storage.put(tx2, b"key2", b"val2").await.unwrap();
        storage.commit(tx2).await.unwrap();

        assert_eq!(storage.get(b"key1").await.unwrap(), Some(b"val1".to_vec()));
        assert_eq!(storage.get(b"key2").await.unwrap(), Some(b"val2".to_vec()));

        // 4. Rollback to CP
        checkpointer.rollback_to(&cp).await.expect("rollback");

        // 5. Verify state
        assert_eq!(storage.get(b"key1").await.unwrap(), Some(b"val1".to_vec()));
        assert_eq!(storage.get(b"key2").await.unwrap(), None);

        // Sequence number should be 1 (last replayed was seq 0)
        assert_eq!(storage.last_seq_no(), 1);
    }
}
