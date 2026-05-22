//! Native State Checkpointing (Time-Travel Debugging).
//!
//! Enables exact state reconstruction of an SAOS database at any given transaction ID
//! by replaying the Write-Ahead Log (WAL) up to that point.

// ANCHOR:SPEC:WP5.1-CHECKPOINT-001 — Time-Travel Debugging ist aktuell ein STUB.
// WP:WP-5.1 PRIO:3 NEEDS:NONE
// AGENT:07 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-09 DEADLINE:NONE
// ANCHOR:FIXME:WP-5.1-ROLLBACK-STUB STATUS:REVIEW AGENT:02
// Nur Datenstrukturen existieren, kein funktionaler Rollback.
// PLAN: WAL bis checkpoint.tx_id replayed → deterministischer State-Restore.
// ABHAENGIGKEIT: Braucht WAL-Ref (aktuell auskommentiert: `wal: Arc<Wal>`).
// SPEC: docs/specs/SPEC-20260505-WP-4.x-Scale.md (State Checkpointing Sektion)

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
    use crate::lsm::LsmConfig;
    use memfuse_core::StorageEngine;
    use std::time::Duration;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_rollback_full_cycle() -> Result<()> {
        let tmp = TempDir::new().expect("temp dir");
        let config = LsmConfig {
            path: tmp.path().to_path_buf(),
            memtable_size_limit: 1024 * 1024,
            max_ram_mb: 64,
            tx_timeout: Duration::from_secs(60),
            compaction: Default::default(),
            encryption_passphrase: None,
        };
        let storage = Arc::new(LsmStorage::new(config).await?);
        let checkpointer = Checkpointer::new(Arc::clone(&storage));

        // 1. Initial State (TX 1-5)
        for i in 1..=5 {
            let tx = TxId::new(i);
            let key = format!("key-{}", i);
            let val = format!("val-{}", i);
            storage.put(tx, key.as_bytes(), val.as_bytes()).await?;
            storage.commit(tx).await?;
        }

        // 2. Take Checkpoint at TX 5
        let checkpoint = checkpointer.create_checkpoint(TxId::new(5));

        // 3. Add more data (TX 6-10)
        for i in 6..=10 {
            let tx = TxId::new(i);
            let key = format!("key-{}", i);
            let val = format!("val-{}", i);
            storage.put(tx, key.as_bytes(), val.as_bytes()).await?;
            storage.commit(tx).await?;
        }

        // Verify data before rollback
        assert!(storage.get(b"key-5").await?.is_some());
        assert!(storage.get(b"key-10").await?.is_some());

        // 4. Rollback to TX 5
        checkpointer.rollback_to(&checkpoint).await?;

        // 5. Verify restored state
        assert!(storage.get(b"key-5").await?.is_some());
        assert!(
            storage.get(b"key-6").await?.is_none(),
            "TX 6 should be rolled back"
        );
        assert!(
            storage.get(b"key-10").await?.is_none(),
            "TX 10 should be rolled back"
        );

        // 6. Verify we can still write after rollback
        let tx_new = TxId::new(11);
        storage.put(tx_new, b"new-key", b"new-val").await?;
        storage.commit(tx_new).await?;
        assert_eq!(storage.get(b"new-key").await?, Some(b"new-val".to_vec()));

        Ok(())
    }
}
