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

    /// Replays all audit entries for a given task via scan_prefix.
    pub async fn replay_task(&self, task_id: &str) -> Result<Vec<AuditEntry>> {
        let prefix = format!("audit:{}:step:", task_id);
        let raw = self.collection.scan_prefix(&prefix).await?;

        let mut entries: Vec<AuditEntry> = raw
            .into_iter()
            .filter_map(|(_key, meta)| {
                let entry_val = meta.get("metadata").cloned().unwrap_or(meta);
                serde_json::from_value::<AuditEntry>(entry_val)
                    .map_err(|e| {
                        tracing::warn!("AuditLog: Deserialisierungsfehler: {e}");
                    })
                    .ok()
            })
            .collect();

        // Sortiere nach step_count für deterministisches Replay
        entries.sort_by_key(|e| e.step_count);
        Ok(entries)
    }
}
