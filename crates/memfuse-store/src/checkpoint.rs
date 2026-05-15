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
// ABHAENGIGKEIT: Braucht WAL-Ref.
// SPEC: docs/specs/SPEC-20260505-WP-4.x-Scale.md (State Checkpointing Sektion)

use crate::wal::{Wal, WalEntry, WalOp};
use bytes::Bytes;
use memfuse_core::{Result, TOMBSTONE_BIT, TxId};
use std::sync::Arc;

/// Represents a Point-in-Time snapshot of the agent's memory state.
#[derive(Debug, Clone)]
pub struct StateCheckpoint {
    pub tx_id: TxId,
    pub timestamp_ms: u64,
}

/// The Checkpointer manages WAL replay bounds for deterministic time-travel.
pub struct Checkpointer {
    wal: Arc<Wal>,
    memtable: Arc<crate::memtable::MemTable>,
}

impl Checkpointer {
    /// Creates a new Checkpointer.
    pub fn new(wal: Arc<Wal>, memtable: Arc<crate::memtable::MemTable>) -> Self {
        Self { wal, memtable }
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

        // 1. Replay WAL
        let wal_entries = self.wal.replay().await?;

        // 2. Clear current MemTable to ensure newer entries don't shadow replayed ones.
        self.memtable.clear();

        for (lsn, entry) in wal_entries {
            let entry_tx_id = match &entry.op {
                WalOp::Put { tx_id, .. } => *tx_id,
                WalOp::Delete { tx_id, .. } => *tx_id,
            };

            if entry_tx_id.inner() > checkpoint.tx_id.inner() {
                break;
            }

            match entry.op {
                WalOp::Put { key, value, .. } => {
                    self.memtable
                        .put(Bytes::from(key), Bytes::from(value), lsn);
                }
                WalOp::Delete { key, .. } => {
                    self.memtable
                        .put(Bytes::from(key), Bytes::new(), lsn | TOMBSTONE_BIT);
                }
            }
        }

        Ok(())
    }
}
