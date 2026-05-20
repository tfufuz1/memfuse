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

use crate::wal::Wal;
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
    wal: Arc<Wal>,
}

impl Checkpointer {
    /// Creates a new Checkpointer.
    pub fn new(wal: Arc<Wal>) -> Self {
        Self { wal }
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
    /// Returns a new MemTable representing the reconstructed state.
    pub async fn rollback_to(
        &self,
        checkpoint: &StateCheckpoint,
    ) -> Result<crate::memtable::MemTable> {
        tracing::info!(
            "Initiating Time-Travel Rollback to TX: {}",
            checkpoint.tx_id
        );

        let entries = self.wal.replay().await?;
        let memtable = crate::memtable::MemTable::new();

        use crate::wal::WalOp;
        use bytes::Bytes;
        use memfuse_core::TOMBSTONE_BIT;

        for (seq_no, entry) in entries {
            let entry_tx_id = match &entry.op {
                WalOp::Put { tx_id, .. } => *tx_id,
                WalOp::Delete { tx_id, .. } => *tx_id,
            };

            // Only apply entries up to the checkpoint's transaction ID
            if entry_tx_id <= checkpoint.tx_id {
                match entry.op {
                    WalOp::Put { key, value, .. } => {
                        memtable.put(Bytes::from(key), Bytes::from(value), seq_no);
                    }
                    WalOp::Delete { key, .. } => {
                        memtable.put(Bytes::from(key), Bytes::new(), seq_no | TOMBSTONE_BIT);
                    }
                }
            }
        }

        Ok(memtable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wal::{WalEntry, WalOp};
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_checkpoint_rollback_deterministic() {
        let tmp = TempDir::new().expect("temp dir"); // #[cfg(test)]
        let wal_path = tmp.path().join("wal.log");
        let wal = Arc::new(Wal::open(&wal_path).await.expect("open wal")); // #[cfg(test)]
        let checkpointer = Checkpointer::new(Arc::clone(&wal));

        let integrity_key = b"memfuse-integrity-key-v1\0\0\0\0\0\0\0\0";

        // 1. Step: Write TX 1 & 2
        let op1 = WalOp::Put {
            tx_id: TxId::new(1),
            key: b"key1".to_vec(),
            value: b"val1".to_vec(),
        };
        wal.append(&WalEntry::try_new(op1, 1, integrity_key).expect("entry 1")) // #[cfg(test)]
            .await
            .expect("append 1"); // #[cfg(test)]

        let op2 = WalOp::Put {
            tx_id: TxId::new(2),
            key: b"key2".to_vec(),
            value: b"val2".to_vec(),
        };
        wal.append(&WalEntry::try_new(op2, 2, integrity_key).expect("entry 2")) // #[cfg(test)]
            .await
            .expect("append 2"); // #[cfg(test)]

        // Create checkpoint at TX 2
        let checkpoint = checkpointer.create_checkpoint(TxId::new(2));

        // 2. Step: Write TX 3 (should be rolled back)
        let op3 = WalOp::Put {
            tx_id: TxId::new(3),
            key: b"key1".to_vec(),
            value: b"val1-updated".to_vec(),
        };
        wal.append(&WalEntry::try_new(op3, 3, integrity_key).expect("entry 3")) // #[cfg(test)]
            .await
            .expect("append 3"); // #[cfg(test)]

        // 3. Rollback
        let rolled_back_memtable = checkpointer
            .rollback_to(&checkpoint)
            .await
            .expect("rollback"); // #[cfg(test)]

        // 4. Verification
        let val1 = rolled_back_memtable.get(b"key1").expect("key1 exists"); // #[cfg(test)]
        assert_eq!(val1.0.as_ref(), b"val1"); // Not "val1-updated"

        let val2 = rolled_back_memtable.get(b"key2").expect("key2 exists"); // #[cfg(test)]
        assert_eq!(val2.0.as_ref(), b"val2");

        assert!(rolled_back_memtable.get(b"key3").is_none());
    }
}
