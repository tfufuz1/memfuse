use memfuse_checkpoint::CheckpointManager;
use memfuse_core::{StorageEngine, TxId};
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;

/// Helper to generate namespaced keys consistent with memfuse-db Collection
fn namespaced_key(col_name: &str, key: &[u8], key_type: u8) -> Vec<u8> {
    let prefix = format!("__col:{}:\x00", col_name).into_bytes();
    let mut k = Vec::with_capacity(prefix.len() + 1 + key.len());
    k.extend_from_slice(&prefix);
    k.push(key_type);
    k.extend_from_slice(key);
    k
}

// ANCHOR:TEST:LAYER-001 — DAG Integrationstest
// WP:NONE PRIO:3 NEEDS:NONE
// AGENT:12 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-09 DEADLINE:NONE
// ZIEL: memfuse-checkpoint -> memfuse-db (Fork + Diverge + Merge)
#[tokio::test]
async fn test_layer_001_checkpoint_fork_diverge() {
    let tmp = TempDir::new().expect("temp dir");
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        ..Default::default()
    };
    let storage = Arc::new(LsmStorage::new(config).await.expect("storage init"));
    let manager = CheckpointManager::new(storage.clone());

    let col_name = "test-col";
    let doc_id_str = "doc-1";
    let user_key = namespaced_key(col_name, doc_id_str.as_bytes(), 0);

    // 1. Phase: Insert (Base State)
    let tx1 = TxId::new(1);
    let base_data = json!({"text": "base version"}).to_string();
    storage
        .put(tx1, &user_key, base_data.as_bytes())
        .await
        .expect("put base");
    storage.commit(tx1).await.expect("commit base");

    let val = storage.get(&user_key).await.expect("get");
    assert_eq!(val, Some(base_data.as_bytes().to_vec()));

    // 2. Phase: Fork (Checkpoint)
    let cp_base = manager
        .create_checkpoint("cp-base")
        .await
        .expect("create checkpoint");

    // Verify seq_no is pinned in snapshot registry
    let min_active = storage.snapshot_registry.min_active_seqno();
    assert!(
        min_active <= cp_base.seq_no,
        "Snapshot registry should pin at or below checkpoint seq_no. Min: {}, CP: {}",
        min_active,
        cp_base.seq_no
    );

    // 3. Phase: Diverge
    let tx2 = TxId::new(2);
    let diverged_data = json!({"text": "diverged version"}).to_string();
    storage
        .put(tx2, &user_key, diverged_data.as_bytes())
        .await
        .expect("put diverged");
    storage.commit(tx2).await.expect("commit diverged");

    // 4. Phase: Verify
    let val_after = storage.get(&user_key).await.expect("get after");
    assert_eq!(
        val_after,
        Some(diverged_data.as_bytes().to_vec()),
        "Storage should return newest data"
    );

    // The checkpoint should still pin the old sequence number
    let min_active_after = storage.snapshot_registry.min_active_seqno();
    assert!(
        min_active_after <= cp_base.seq_no,
        "Checkpoint must still pin the base sequence number after divergence"
    );

    // Cleanup
    manager.drop_checkpoint(&cp_base).await.expect("drop cp");
    // After drop, if no other snapshots exist, min_active should move up
    let min_active_final = storage.snapshot_registry.min_active_seqno();
    assert!(
        min_active_final > cp_base.seq_no,
        "After dropping checkpoint, min_active should be greater than checkpoint seq_no"
    );
}
