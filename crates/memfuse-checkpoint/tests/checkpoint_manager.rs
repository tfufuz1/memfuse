// ANCHOR:INTEGRATION:CHECKPOINT-001 STATUS:DONE AGENT:12 DATE:2026-06-21
//! Integration tests for CheckpointManager with real LSM storage.

use memfuse_checkpoint::{CheckpointManager, CheckpointRegistry};
use memfuse_store::{LsmStorage, LsmConfig};
use memfuse_core::{TxId, WorkflowState};
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_checkpoint_manager_persistence_lifecycle() {
    let tmp = TempDir::new().unwrap();
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        ..Default::default()
    };
    let storage = Arc::new(LsmStorage::new(config).await.expect("Failed to open storage"));

    let manager = CheckpointManager::new(storage.clone());

    // 1. Create a checkpoint
    let meta = manager.create_checkpoint(
        "stable_v1",
        "collection_alpha",
        42,
        serde_json::json!({"version": 1, "tags": ["prod"]})
    ).await.expect("Failed to create checkpoint");

    assert_eq!(meta.name, "stable_v1");
    assert_eq!(meta.seq_no, 42);

    // 2. Verify it's in the list
    let list = manager.list_checkpoints().await.expect("Failed to list");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "stable_v1");

    // 3. Close and reopen (simulate persistence)
    // We drop the manager and create a new one using the same storage
    let manager_reopened = CheckpointManager::new(storage.clone());

    // Explicitly reload to verify it picks up from LSM
    manager_reopened.reload_from_storage().await.expect("Failed to reload");

    let meta_reopened = manager_reopened.get_checkpoint("stable_v1").await.expect("Failed to get")
        .expect("Checkpoint should exist after reload");

    assert_eq!(meta_reopened.seq_no, 42);
    assert_eq!(meta_reopened.collection_id, "collection_alpha");

    // 4. Drop checkpoint
    manager_reopened.drop_checkpoint("stable_v1").await.expect("Failed to drop");
    let list_after = manager_reopened.list_checkpoints().await.unwrap();
    assert!(list_after.is_empty(), "Checkpoint list should be empty after drop");
}

#[test]
fn test_in_memory_registry_sync() {
    let registry = CheckpointRegistry::new();
    let tx = TxId::new(999);
    let state = WorkflowState {
        tx,
        graph_hash: "abc-123".to_string(),
    };

    registry.register(tx, state.clone());

    let retrieved = registry.get(tx).expect("Should find registered state");
    assert_eq!(retrieved.graph_hash, "abc-123");
}
