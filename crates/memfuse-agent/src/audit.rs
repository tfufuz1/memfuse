//! Immutable audit trail for agent workflow executions.
//!
//! Provides append-only logging of every step an agent takes.
//! Entries are stored via [`Collection`] and keyed `audit:{task_id}:step:{n}`.

use memfuse_core::Result;
use memfuse_db::Collection;
use memfuse_store::LsmStorage;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Single immutable record of an agent step execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub task_id: String,
    pub step_count: u64,
    pub node_id: String,
    pub tokens_consumed: usize,
    pub payload: serde_json::Value,
}

/// Append-only audit log backed by a MemFuse collection.
pub struct AuditLog {
    collection: Arc<Collection<LsmStorage>>,
}

impl AuditLog {
    pub fn new(collection: Arc<Collection<LsmStorage>>) -> Self {
        Self { collection }
    }

    /// Appends an immutable audit entry. No delete/update path exists by design (AC-3).
    pub async fn append(&self, entry: &AuditEntry) -> Result<()> {
        let audit_id = format!("audit:{}:step:{}", entry.task_id, entry.step_count);
        let payload = serde_json::to_value(entry)
            .map_err(|e| memfuse_core::MemFuseError::Internal(e.to_string()))?;

        let dummy_vec = vec![0.0; self.collection.dimension()];
        self.collection
            .insert(&audit_id, &dummy_vec, Some(payload))
            .await
    }

    /// Replays all audit entries for a given task by sequentially probing keys.
    ///
    /// Since `Collection` has no prefix-scan API, we probe
    /// `audit:{task_id}:step:0`, `audit:{task_id}:step:1`, … until a gap is found.
    pub async fn replay_task(&self, task_id: &str) -> Result<Vec<AuditEntry>> {
        let mut entries = Vec::new();
        for step in 0u64.. {
            let key = format!("audit:{}:step:{}", task_id, step);
            match self.collection.get(&key).await? {
                Some(doc) => {
                    if let Some(meta) = doc.metadata {
                        let entry: AuditEntry = serde_json::from_value(meta)
                            .map_err(|e| memfuse_core::MemFuseError::Internal(e.to_string()))?;
                        entries.push(entry);
                    }
                }
                None => break,
            }
        }
        Ok(entries)
    }
}
