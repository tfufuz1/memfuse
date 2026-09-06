// FILE-CONTEXT Header (Format v3)
// ZWECK: Append-only immutable audit trail logging for agent state transitions.
// INVARIANTEN: Keyed `audit:{task_id}:step:{n}`; zero deletion/update paths by design.
// NICHT-OFFENSICHTLICH: replay_task scans prefix and sorts by step_count for deterministic replay.
// HOTSPOTS: append (ll. 35-50), replay_task (ll. 52-70).
// STAND: TS:2026-09-01T23:11:04Z (SESSION: 5a38054a)

//! Immutable audit trail for agent workflow executions.
//!
//! Provides append-only logging of every step an agent takes.
//! Entries are stored via [`Collection`] and keyed `audit:{task_id}:step:{n}`.

use crate::context::{validate_node_id, validate_task_id};
use memfuse_core::{
    Result, StorageEngine};
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Append-only audit log backed by a MemFuse collection.
pub struct AuditLog<S: StorageEngine = LsmStorage> {
    collection: Arc<Collection<S>>,
}

/// Validates payload and optional error string of an audit entry for non-emptiness and absence of null bytes.
pub fn validate_audit_payload_and_error(
    payload: &serde_json::Value,
    error: Option<&str>,
) -> Result<()> {
    if let Some(err) = error {
        if err.trim().is_empty() {
            return Err(memfuse_core::MemFuseError::InvalidInput(
                "Audit error message cannot be empty when provided".to_string(),
            ));
        }
        if err.contains('\0') {
            return Err(memfuse_core::MemFuseError::InvalidInput(
                "Audit error message cannot contain null bytes".to_string(),
            ));
        }
    }

    if let Some(s) = payload.as_str() {
        if s.trim().is_empty() {
            return Err(memfuse_core::MemFuseError::InvalidInput(
                "Audit string payload cannot be empty".to_string(),
            ));
        }
    }

    let json_str = payload.to_string();
    if json_str.contains('\0') || json_str.contains("\\u0000") {
        return Err(memfuse_core::MemFuseError::InvalidInput(
            "Audit payload cannot contain null bytes".to_string(),
        ));
    }

    Ok(())
}

impl<S: StorageEngine> AuditLog<S> {
    pub fn new(collection: Arc<Collection<S>>) -> Self {
        Self { collection }
    }

    /// Appends an immutable audit entry directly to a collection without needing an [`AuditLog`] wrapper.
    pub async fn append_to(collection: &Collection<S>, entry: &AuditEntry) -> Result<()> {
        validate_task_id(&entry.task_id)?;
        validate_node_id(&entry.node_id)?;
        validate_audit_payload_and_error(&entry.payload, entry.error.as_deref())?;

        let audit_id = format!("audit:{}:step:{}", entry.task_id, entry.step_count);
        let payload = serde_json::to_value(entry)
            .map_err(|e| memfuse_core::MemFuseError::Internal(e.to_string()))?;

        collection.put_kv_if_absent(&audit_id, &payload).await
    }

    /// Appends an immutable audit entry directly via LSM storage without HNSW vector index participation (AC-3).
    /// Enforces append-only immutability: returns `MemFuseError::Conflict` if an entry for the step already exists.
    /// A `Conflict` error indicates that a step with this ID already exists - the caller must choose a new `step_count`
    /// (e.g. using `next_retry_step_count()`), and NOT automatically overwrite.
    pub async fn append(&self, entry: &AuditEntry) -> Result<()> {
        Self::append_to(&self.collection, entry).await
    }

    /// Replays all audit entries for a given task via scan_prefix.
    pub async fn replay_task(&self, task_id: &str) -> Result<Vec<AuditEntry>> {
        validate_task_id(task_id)?;

        let prefix = format!("audit:{}:step:", task_id);
        let raw = self.collection.scan_prefix(&prefix).await?;

        let mut entries: Vec<AuditEntry> = raw
            .into_iter()
            .filter_map(|(_key, meta)| {
                let entry_val = meta.get("metadata").cloned().unwrap_or(meta);
                serde_json::from_value::<AuditEntry>(entry_val)
                    .map_err(|e| {
                        tracing::warn!("AuditLog: Deserialization error: {e}");
                    })
                    .ok()
            })
            .collect();

        // Sort by step_count for deterministic replay
        entries.sort_by_key(|e| e.step_count);
        Ok(entries)
    }
}

/// Migrates legacy zero-vector audit entries from the HNSW vector index and doc_key mappings
/// into the direct LSM KV store format (`put_kv`).
///
/// Returns the number of migrated audit entries.
pub async fn migrate_legacy_audit_entries<S: StorageEngine, V: memfuse_core::VectorIndex>(
    collection: &Collection<S, V>,
) -> Result<usize> {
    let raw = collection.scan_prefix("audit:").await?;
    let mut count = 0;

    for (key, val) in raw {
        if !key.starts_with("audit:") {
            continue;
        }

        // Check if value was stored in legacy StoredDocument format (contains "embedding" field)
        if let Some(obj) = val.as_object() {
            if obj.contains_key("embedding") {
                // Extract audit entry payload from metadata
                let entry_val = obj.get("metadata").cloned().unwrap_or(val.clone());

                let doc_id = memfuse_core::DocId::from_key(&key)?;
                let tx = collection.allocate_tx()?;

                // Remove from HNSW vector index
                let _ = collection.vector_index().delete(tx, doc_id).await;
                let _ = collection.vector_index().commit(tx).await;

                // Delete legacy doc_key (key_type=1) mapping
                let doc_key = collection.namespaced_key(&doc_id.inner().to_le_bytes(), 1);
                collection.storage().delete(tx, &doc_key).await?;

                // Save pure KV entry
                collection.put_kv(&key, &entry_val).await?;

                collection.storage().commit(tx).await?;
                count += 1;
            }
        }
    }

    if count > 0 {
        tracing::info!(
            "Migrated {} legacy zero-vector audit entries from HNSW index",
            count
        );
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use memfuse_core::BoxFuture;
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
            error: None,
        };

        let entry2 = AuditEntry {
            task_id: "task-123".to_string(),
            step_count: 2,
            node_id: "node-process".to_string(),
            tokens_consumed: 120,
            payload: serde_json::json!({"action": "compute"}),
            error: None,
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

    #[tokio::test]
    async fn test_audit_log_duplicate_step_rejected() {
        let storage = Arc::new(InMemoryStorageEngine::new());
        let index = Arc::new(HnswIndex::try_new(HnswConfig::default()).unwrap());
        let graph_index = Arc::new(CsrGraph::new());
        let next_tx = Arc::new(std::sync::atomic::AtomicU64::new(1));

        let collection = Arc::new(Collection::new(
            "test_audit_dup".to_string(),
            storage,
            index,
            graph_index,
            next_tx,
            1536,
            memfuse_text::Language::English,
        ));

        let audit_log = AuditLog::new(collection);

        let entry = AuditEntry {
            task_id: "task-dup".to_string(),
            step_count: 1,
            node_id: "node-1".to_string(),
            tokens_consumed: 50,
            payload: serde_json::json!({"step": 1}),
            error: None,
        };

        audit_log.append(&entry).await.unwrap();

        let dup_res = audit_log.append(&entry).await;
        assert!(
            matches!(dup_res, Err(memfuse_core::MemFuseError::Conflict(_))),
            "Expected Conflict error on duplicate audit step append"
        );
    }

    #[tokio::test]
    async fn test_audit_log_null_byte_and_empty_input_boundary_guards() {
        let storage = Arc::new(InMemoryStorageEngine::new());
        let index = Arc::new(HnswIndex::try_new(HnswConfig::default()).unwrap());
        let graph_index = Arc::new(CsrGraph::new());
        let next_tx = Arc::new(std::sync::atomic::AtomicU64::new(1));

        let collection = Arc::new(Collection::new(
            "test_audit_guards".to_string(),
            storage,
            index,
            graph_index,
            next_tx,
            1536,
            memfuse_text::Language::English,
        ));

        let audit_log = AuditLog::new(collection);

        // 1. Empty and null-byte task_ids
        let task_ids_to_test = vec!["", "\0", "\0\0", "\0task", "task\0id", "task\0"];

        for task_id in &task_ids_to_test {
            let entry = AuditEntry {
                task_id: task_id.to_string(),
                step_count: 1,
                node_id: "valid_node".to_string(),
                tokens_consumed: 10,
                payload: serde_json::json!({"ok": true}),
                error: None,
            };
            assert!(
                matches!(
                    audit_log.append(&entry).await,
                    Err(memfuse_core::MemFuseError::InvalidInput(_))
                ),
                "Expected InvalidInput for task_id: {:?}",
                task_id
            );
            assert!(
                matches!(
                    audit_log.replay_task(task_id).await,
                    Err(memfuse_core::MemFuseError::InvalidInput(_))
                ),
                "Expected InvalidInput for replay_task with task_id: {:?}",
                task_id
            );
        }

        // 2. Empty and null-byte node_ids
        let node_ids_to_test = vec!["", "\0", "\0node", "node\0id", "node\0"];

        for node_id in &node_ids_to_test {
            let entry = AuditEntry {
                task_id: "valid_task".to_string(),
                step_count: 1,
                node_id: node_id.to_string(),
                tokens_consumed: 10,
                payload: serde_json::json!({"ok": true}),
                error: None,
            };
            assert!(
                matches!(
                    audit_log.append(&entry).await,
                    Err(memfuse_core::MemFuseError::InvalidInput(_))
                ),
                "Expected InvalidInput for node_id: {:?}",
                node_id
            );
        }

        // 3. Empty string and null-byte payloads
        let invalid_payloads = vec![
            serde_json::Value::String("".to_string()),
            serde_json::Value::String("\0".to_string()),
            serde_json::Value::String("\0payload".to_string()),
            serde_json::Value::String("pay\0load".to_string()),
            serde_json::Value::String("payload\0".to_string()),
            serde_json::json!({"nested": "val\0null"}),
        ];

        for payload in invalid_payloads {
            let entry = AuditEntry {
                task_id: "valid_task".to_string(),
                step_count: 1,
                node_id: "valid_node".to_string(),
                tokens_consumed: 10,
                payload,
                error: None,
            };
            assert!(
                matches!(
                    audit_log.append(&entry).await,
                    Err(memfuse_core::MemFuseError::InvalidInput(_))
                ),
                "Expected InvalidInput for payload: {:?}",
                entry.payload
            );
        }

        // 4. Empty and null-byte error messages
        let invalid_errors = vec!["", "\0", "\0err", "err\0or", "error\0"];

        for err_msg in invalid_errors {
            let entry = AuditEntry {
                task_id: "valid_task".to_string(),
                step_count: 1,
                node_id: "valid_node".to_string(),
                tokens_consumed: 0,
                payload: serde_json::Value::Null,
                error: Some(err_msg.to_string()),
            };
            assert!(
                matches!(
                    audit_log.append(&entry).await,
                    Err(memfuse_core::MemFuseError::InvalidInput(_))
                ),
                "Expected InvalidInput for error: {:?}",
                err_msg
            );
        }
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
impl StorageEngine for InMemoryStorageEngine {
    fn get<'a>(&'a self, key: &'a [u8]) -> BoxFuture<'a, Result<Option<Vec<u8>>>> {
        Box::pin(async move {
        let guard = self
            .data
            .lock()
            .map_err(|e| memfuse_core::MemFuseError::Internal(format!("Lock poisoned: {e}")))?;
        Ok(guard.get(key).cloned())
        })
    }

    fn get_at_seq<'a>(&'a self, key: &'a [u8], _seq: u64) -> BoxFuture<'a, Result<Option<Vec<u8>>>> {
        Box::pin(async move {
        self.get(key).await
        })
    }

    fn put<'a>(&'a self, _tx_id: memfuse_core::TxId, key: &'a [u8], value: &'a [u8]) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
        let mut guard = self
            .data
            .lock()
            .map_err(|e| memfuse_core::MemFuseError::Internal(format!("Lock poisoned: {e}")))?;
        guard.insert(key.to_vec(), value.to_vec());
        Ok(())
        })
    }

    fn delete<'a>(&'a self, _tx_id: memfuse_core::TxId, key: &'a [u8]) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
        let mut guard = self
            .data
            .lock()
            .map_err(|e| memfuse_core::MemFuseError::Internal(format!("Lock poisoned: {e}")))?;
        guard.remove(key);
        Ok(())
        })
    }

    fn commit<'a>(&'a self, _tx_id: memfuse_core::TxId) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
        Ok(())
        })
    }

    fn rollback<'a>(&'a self, _tx_id: memfuse_core::TxId) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
        Ok(())
        })
    }

    fn rollback_to_tx<'a>(&'a self, _tx_id: memfuse_core::TxId) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
        Ok(())
        })
    }

    fn flush<'a>(&'a self) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
        Ok(())
        })
    }

    fn stats<'a>(&'a self) -> BoxFuture<'a, Result<memfuse_core::StorageStats>> {
        Box::pin(async move {
        Ok(memfuse_core::StorageStats {
            num_segments: 0,
            total_size_bytes: 0,
            memtable_size_bytes: 0,
        })
        })
    }

    fn last_seq_no<'a>(&'a self) -> BoxFuture<'a, Result<u64>> {
        Box::pin(async move {
        Ok(0)
        })
    }

    fn last_tx_id<'a>(&'a self) -> BoxFuture<'a, Result<memfuse_core::TxId>> {
        Box::pin(async move {
        Ok(memfuse_core::TxId::new(0))
        })
    }

    fn pin_checkpoint<'a>(&'a self, _seq_no: u64) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
        Ok(())
        })
    }

    fn unpin_checkpoint<'a>(&'a self, _seq_no: u64) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
        Ok(())
        })
    }

    fn scan_prefix<'a>(&'a self, prefix: &'a [u8]) -> BoxFuture<'a, Result<Vec<(Vec<u8>, Vec<u8>)>>> {
        Box::pin(async move {
        let guard = self
            .data
            .lock()
            .map_err(|e| memfuse_core::MemFuseError::Internal(format!("Lock poisoned: {e}")))?;
        let entries = guard
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        Ok(entries)
        })
    }

    fn scan<'a>(
        &'a self,
        start: std::ops::Bound<&'a [u8]>,
        end: std::ops::Bound<&'a [u8]>,
    ) -> BoxFuture<'a, Result<Vec<(Vec<u8>, Vec<u8>)>>> {
        Box::pin(async move {
        let guard = self
            .data
            .lock()
            .map_err(|e| memfuse_core::MemFuseError::Internal(format!("Lock poisoned: {e}")))?;
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
        })
    }
}
