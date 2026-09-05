// FILE-CONTEXT Header (Format v3)
// ZWECK: Persistent dead-letter queue storage abstraction for failed agent steps.
// INVARIANTEN: Key prefix "dlq:" for LSM prefix isolation; atomic put/delete operations.
// NICHT-OFFENSICHTLICH: drain reads all DLQ entries and deletes them atomically in a single batch transaction.
// HOTSPOTS: push (ll. 25-45), drain (ll. 50-80).
// STAND: TS:2026-09-03T00:00:00Z

//! Persistent Dead-Letter-Queue for failed agent step executions.

use crate::step::StepDeadLetter;
use memfuse_core::traits::StorageEngine;
use memfuse_core::{MemFuseError, Result, TxId};
use std::sync::Arc;

/// Persistente Dead-Letter-Queue für fehlgeschlagene Agent-Schritte.
/// Verwendet denselben Storage wie der Agent (LSM) mit einem fixen Key-Prefix.
pub struct DeadLetterQueue {
    storage: Arc<dyn StorageEngine>,
}

impl DeadLetterQueue {
    pub const PREFIX: &'static [u8] = b"dlq:";

    pub fn new(storage: Arc<dyn StorageEngine>) -> Self {
        Self { storage }
    }

    pub async fn push(&self, letter: &StepDeadLetter) -> Result<()> {
        let key = format!(
            "dlq:{}:{}:{}:{}",
            letter.session_id, letter.failed_at_secs, letter.node_id, letter.attempt
        );
        let value =
            serde_json::to_vec(letter).map_err(|e| MemFuseError::Serialization(e.to_string()))?;

        let tx = self.allocate_tx().await?;
        if let Err(e) = self.storage.put(tx, key.as_bytes(), &value).await {
            let _ = self.storage.rollback(tx).await;
            return Err(e);
        }
        if let Err(e) = self.storage.commit(tx).await {
            let _ = self.storage.rollback(tx).await;
            return Err(e);
        }
        Ok(())
    }

    pub async fn drain(&self) -> Result<Vec<StepDeadLetter>> {
        let entries = self.storage.scan_prefix(Self::PREFIX).await?;
        let mut letters = Vec::with_capacity(entries.len());
        let mut keys_to_delete = Vec::with_capacity(entries.len());

        for (key, val) in entries {
            let letter: StepDeadLetter = serde_json::from_slice(&val)
                .map_err(|e| MemFuseError::Serialization(e.to_string()))?;
            letters.push(letter);
            keys_to_delete.push(key);
        }

        if !keys_to_delete.is_empty() {
            let tx = self.allocate_tx().await?;
            if let Err(e) = self.storage.delete_many(tx, keys_to_delete).await {
                let _ = self.storage.rollback(tx).await;
                return Err(e);
            }
            if let Err(e) = self.storage.commit(tx).await {
                let _ = self.storage.rollback(tx).await;
                return Err(e);
            }
        }

        Ok(letters)
    }

    pub async fn list(&self) -> Result<Vec<StepDeadLetter>> {
        let entries = self.storage.scan_prefix(Self::PREFIX).await?;
        let mut letters = Vec::with_capacity(entries.len());

        for (_key, val) in entries {
            let letter: StepDeadLetter = serde_json::from_slice(&val)
                .map_err(|e| MemFuseError::Serialization(e.to_string()))?;
            letters.push(letter);
        }

        Ok(letters)
    }

    async fn allocate_tx(&self) -> Result<TxId> {
        let last_tx = self.storage.last_tx_id().await?.0;
        Ok(TxId::new(last_tx + 1))
    }
}
