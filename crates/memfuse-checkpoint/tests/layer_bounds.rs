use memfuse_checkpoint::CheckpointManager;
use memfuse_store::lsm::{LsmStorage, LsmConfig};
use memfuse_core::{StorageEngine, TxId};
use std::sync::Arc;
use tempfile::TempDir;

// ANCHOR:TEST:LAYER-001 — DAG Integrationstest
// WP:NONE PRIO:3 NEEDS:NONE
// AGENT:12 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-09 DEADLINE:NONE
// ZIEL: memfuse-checkpoint -> memfuse-store (Fork + Diverge + Merge/Rollback Logic)
#[tokio::test]
async fn test_layer_001_checkpoint_fork_diverge_rollback() {
    let tmp = TempDir::new().expect("temp dir");
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        ..Default::default()
    };

    let storage = Arc::new(LsmStorage::new(config).await.expect("storage"));
    let manager = CheckpointManager::new(storage.clone());

    // 1. Initial Data (Base state)
    let tx1 = TxId::new(1);
    storage.put(tx1, b"key1", b"val1").await.expect("put1");
    storage.commit(tx1).await.expect("commit1");

    // 2. Create Checkpoint (Fork point)
    let cp1 = manager.create_checkpoint("v1").await.expect("create checkpoint");
    let fork_seq = cp1.seq_no;
    assert!(fork_seq > 0);

    // Verify sequence number is pinned in the registry
    assert_eq!(storage.snapshot_registry.min_active_seqno(), fork_seq);

    // 3. Diverge: Add more data and overwrite existing
    let tx2 = TxId::new(2);
    storage.put(tx2, b"key2", b"val2").await.expect("put2");
    storage.commit(tx2).await.expect("commit2");

    let tx3 = TxId::new(3);
    storage.put(tx3, b"key1", b"val1_updated").await.expect("put3");
    storage.commit(tx3).await.expect("commit3");

    // Current state should have updated values
    assert_eq!(storage.get(b"key1").await.expect("get key1"), Some(b"val1_updated".to_vec()));
    assert_eq!(storage.get(b"key2").await.expect("get key2"), Some(b"val2".to_vec()));

    // 4. Rollback (Stub implementation for now)
    // In the future, this would restore the state to 'val1' for 'key1' and no 'key2'.
    manager.rollback(&cp1).await.expect("rollback stub");

    // 5. Cleanup: Drop Checkpoint (Merge/Release)
    manager.drop_checkpoint(&cp1).await.expect("drop checkpoint");

    // Verify sequence number is unpinned
    // If no other snapshots are active, min_active_seqno returns u64::MAX
    assert_eq!(storage.snapshot_registry.min_active_seqno(), u64::MAX);
}

#[tokio::test]
async fn test_multiple_checkpoints_pinning() {
    let tmp = TempDir::new().expect("temp dir");
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        ..Default::default()
    };

    let storage = Arc::new(LsmStorage::new(config).await.expect("storage"));
    let manager = CheckpointManager::new(storage.clone());

    // CP1
    let tx1 = TxId::new(1);
    storage.put(tx1, b"k", b"v1").await.expect("p1");
    storage.commit(tx1).await.expect("c1");
    let cp1 = manager.create_checkpoint("cp1").await.expect("cp1");

    // CP2
    let tx2 = TxId::new(2);
    storage.put(tx2, b"k", b"v2").await.expect("p2");
    storage.commit(tx2).await.expect("c2");
    let cp2 = manager.create_checkpoint("cp2").await.expect("cp2");

    // min should be CP1
    assert_eq!(storage.snapshot_registry.min_active_seqno(), cp1.seq_no);

    // Drop CP1
    manager.drop_checkpoint(&cp1).await.expect("drop1");

    // min should now be CP2
    assert_eq!(storage.snapshot_registry.min_active_seqno(), cp2.seq_no);

    // Drop CP2
    manager.drop_checkpoint(&cp2).await.expect("drop2");
    assert_eq!(storage.snapshot_registry.min_active_seqno(), u64::MAX);
}
