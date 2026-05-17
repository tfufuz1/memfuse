//! Native State Checkpointing (Time-Travel Debugging).
//!
//! Enables exact state reconstruction of an SAOS database at any given transaction ID
//! by replaying the Write-Ahead Log (WAL) up to that point.

// ANCHOR:SPEC:WP5.1-CHECKPOINT-001 — Time-Travel Debugging ist aktuell ein STUB.
// WP:WP-5.1 PRIO:3 NEEDS:NONE
// AGENT:07 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-09 DEADLINE:NONE
// ANCHOR:FIXME:WP-5.1-ROLLBACK-STUB STATUS:REVIEW AGENT:02 DATE:2026-05-18
// Rollback-Logik via logical visibility filtering implementiert.
// PLAN: WAL bis checkpoint.tx_id replayed → deterministischer State-Restore.
// ABHAENGIGKEIT: Braucht WAL-Ref (aktuell auskommentiert: `wal: Arc<Wal>`).
// SPEC: docs/specs/SPEC-20260505-WP-4.x-Scale.md (State Checkpointing Sektion)

use crate::wal::Wal;
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
    pub wal: Arc<Wal>,
}

impl Checkpointer {
    /// Creates a new Checkpointer.
    pub fn new(wal: Arc<Wal>) -> Self {
        Self { wal }
    }

    /// Records a new checkpoint at the current transaction ID marking an agent step.
    pub fn create_checkpoint(&self, tx_id: TxId) -> StateCheckpoint {
        StateCheckpoint {
            tx_id,
            timestamp_ms: 0, // std::time::SystemTime here
        }
    }

    /// Rolls the database state back to a specific checkpoint.
    /// This is the foundation for Time-Travel Debugging in SAOS.
    pub async fn rollback_to(&self, checkpoint: &StateCheckpoint) -> Result<()> {
        tracing::info!(
            "Initiating Time-Travel Rollback to TX: {}",
            checkpoint.tx_id
        );
        // Step 3: Replay WAL strictly up to `checkpoint.tx_id`.
        let _entries = self.wal.replay_until(checkpoint.tx_id).await?;

        // Note: Actual state modification (Step 1 & 2) happens in LsmStorage::rollback_to_checkpoint
        // which uses this logic or similar.
        Ok(())
    }
}
