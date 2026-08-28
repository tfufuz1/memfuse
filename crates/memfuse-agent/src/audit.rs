//! Immutable audit trail for agent workflow executions.
//!
//! Provides append-only logging of every step an agent takes.
//! Entries are stored via [`Collection`] and keyed `audit:{task_id}:step:{n}`.

use memfuse_core::{StorageEngine, Result};
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
pub struct AuditLog<S: StorageEngine = LsmStorage> {
    collection: Arc<Collection<S>>,
}

impl<S: StorageEngine> AuditLog<S> {
    pub fn new(collection: Arc<Collection<S>>) -> Self {
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

#[cfg(test)]
mod tests {
    use super::*;
    use memfuse_graph::CsrGraph;
    use memfuse_index::{HnswConfig, HnswIndex};

    #[tokio::test]
    async fn test_audit_log_in_memory_storage() {
        let storage = Arc::new(InMemoryStorageEngine::new());
        let index = Arc::new(HnswIndex::try_new(HnswConfig::default()).unwrap()); // unwrap allowed
        let graph_index = Arc::new(CsrGraph::new());
        let next_tx = Arc::new(std::sync::atomic::AtomicU64::new(1));

        let collection = Arc::new(Collection::new(
            "test_audit".to_string(),
            storage,
            index,
            graph_index,
            next_tx,
            1536,
            memfuse_text::Language::English,
        ));

        let audit_log = AuditLog::new(collection);

        let entry1 = AuditEntry {
            task_id: "task-123".to_string(),
            step_count: 1,
            node_id: "node-start".to_string(),
            tokens_consumed: 50,
            payload: serde_json::json!({"action": "init"}),
        };

        let entry2 = AuditEntry {
            task_id: "task-123".to_string(),
            step_count: 2,
            node_id: "node-process".to_string(),
            tokens_consumed: 120,
            payload: serde_json::json!({"action": "compute"}),
        };

        audit_log.append(&entry1).await.unwrap(); // unwrap allowed
        audit_log.append(&entry2).await.unwrap(); // unwrap allowed

        let replayed = audit_log.replay_task("task-123").await.unwrap(); // unwrap allowed
        assert_eq!(replayed.len(), 2);
        assert_eq!(replayed[0].step_count, 1);
        assert_eq!(replayed[0].node_id, "node-start");
        assert_eq!(replayed[1].step_count, 2);
        assert_eq!(replayed[1].node_id, "node-process");
    }
}

/// Minimal in-memory implementation of [`StorageEngine`] backed by a thread-safe map.
/// Useful for testing and fast mock storage.
#[cfg(any(test, feature = "test-utils"))]
#[derive(Debug, Default)]
pub struct InMemoryStorageEngine {
    data: std::sync::Mutex<std::collections::HashMap<Vec<u8>, Vec<u8>>>,
}

#[cfg(any(test, feature = "test-utils"))]
impl InMemoryStorageEngine {
    pub fn new() -> Self {
        Self {
            data: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }
}

#[cfg(any(test, feature = "test-utils"))]
#[async_trait::async_trait]
impl StorageEngine for InMemoryStorageEngine {
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let guard = self.data.lock().unwrap(); // unwrap allowed
        Ok(guard.get(key).cloned())
    }

    async fn get_at_seq(&self, key: &[u8], _seq: u64) -> Result<Option<Vec<u8>>> {
        self.get(key).await
    }

    async fn put(&self, _tx_id: memfuse_core::TxId, key: &[u8], value: &[u8]) -> Result<()> {
        let mut guard = self.data.lock().unwrap(); // unwrap allowed
        guard.insert(key.to_vec(), value.to_vec());
        Ok(())
    }

    async fn delete(&self, _tx_id: memfuse_core::TxId, key: &[u8]) -> Result<()> {
        let mut guard = self.data.lock().unwrap(); // unwrap allowed
        guard.remove(key);
        Ok(())
    }

    async fn commit(&self, _tx_id: memfuse_core::TxId) -> Result<()> {
        Ok(())
    }

    async fn rollback(&self, _tx_id: memfuse_core::TxId) -> Result<()> {
        Ok(())
    }

    async fn rollback_to_tx(&self, _tx_id: memfuse_core::TxId) -> Result<()> {
        Ok(())
    }

    async fn flush(&self) -> Result<()> {
        Ok(())
    }

    async fn stats(&self) -> Result<memfuse_core::StorageStats> {
        Ok(memfuse_core::StorageStats {
            num_segments: 0,
            total_size_bytes: 0,
            memtable_size_bytes: 0,
        })
    }

    async fn last_seq_no(&self) -> Result<u64> {
        Ok(0)
    }

    async fn last_tx_id(&self) -> Result<memfuse_core::TxId> {
        Ok(memfuse_core::TxId::new(0))
    }

    async fn pin_checkpoint(&self, _seq_no: u64) -> Result<()> {
        Ok(())
    }

    async fn unpin_checkpoint(&self, _seq_no: u64) -> Result<()> {
        Ok(())
    }

    async fn scan_prefix(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let guard = self.data.lock().unwrap(); // unwrap allowed
        let entries = guard
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        Ok(entries)
    }

    async fn scan(
        &self,
        start: std::ops::Bound<&[u8]>,
        end: std::ops::Bound<&[u8]>,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let guard = self.data.lock().unwrap(); // unwrap allowed
        let mut entries: Vec<(Vec<u8>, Vec<u8>)> = guard
            .iter()
            .filter(|(k, _)| {
                let s_ok = match start {
                    std::ops::Bound::Included(s) => k.as_slice() >= s,
                    std::ops::Bound::Excluded(s) => k.as_slice() > s,
                    std::ops::Bound::Unbounded => true,
                };
                let e_ok = match end {
                    std::ops::Bound::Included(e) => k.as_slice() <= e,
                    std::ops::Bound::Excluded(e) => k.as_slice() < e,
                    std::ops::Bound::Unbounded => true,
                };
                s_ok && e_ok
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(entries)
    }
}
