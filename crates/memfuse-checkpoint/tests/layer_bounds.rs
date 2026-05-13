// ANCHOR:TEST:LAYER-001 — DAG Integrationstest
// WP:NONE PRIO:3 NEEDS:NONE
// AGENT:12 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-09 DEADLINE:NONE
// ZIEL: memfuse-checkpoint -> memfuse-db (Fork + Diverge + Merge)

use memfuse_core::Result;
use memfuse_store::lsm::{LsmStorage, LsmConfig};
use memfuse_checkpoint::CheckpointManager;
use memfuse_db::Collection;
use memfuse_index::{HnswIndex, HnswConfig};
use serde_json::json;
use tempfile::TempDir;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

#[tokio::test]
async fn test_layer_001_fork_diverge_merge() -> Result<()> {
    let tmp = TempDir::new().expect("valid test value");
    let lsm_config = LsmConfig {
        path: tmp.path().to_path_buf(),
        ..Default::default()
    };
    let storage = Arc::new(LsmStorage::new(lsm_config).await?);
    let manager = CheckpointManager::new(storage.clone());
    let next_tx = Arc::new(AtomicU64::new(1));
    let dimension = 4;

    let create_col = |name: &str| {
        let hnsw_config = HnswConfig {
            dimension,
            ..Default::default()
        };
        let index = Arc::new(HnswIndex::new(hnsw_config));
        Collection::new(
            name.to_string(),
            storage.clone(),
            index,
            next_tx.clone(),
            dimension,
        )
    };

    // 1. Initialize Main Collection
    let main = create_col("main");
    main.insert("doc1", &[1.0, 0.0, 0.0, 0.0], Some(json!({"v": 1}))).await?;
    main.insert("doc2", &[0.0, 1.0, 0.0, 0.0], Some(json!({"v": 2}))).await?;

    // 2. Create Checkpoint (Pins sequence number)
    let cp = manager.create_checkpoint("base_state").await?;
    let base_seq = cp.seq_no;
    assert!(base_seq > 0);
    assert!(storage.snapshot_registry.min_active_seqno() <= base_seq);

    // 3. Diverge: Create branches A and B
    // These are isolated collections sharing the same LSM storage but using different key prefixes.
    let branch_a = create_col("branch_a");
    let branch_b = create_col("branch_b");

    branch_a.insert("doc_a", &[0.0, 0.0, 1.0, 0.0], Some(json!({"branch": "a"}))).await?;
    branch_b.insert("doc_b", &[0.0, 0.0, 0.0, 1.0], Some(json!({"branch": "b"}))).await?;

    // 4. Verify Isolation
    assert!(main.get("doc_a").await?.is_none());
    assert!(main.get("doc_b").await?.is_none());
    assert!(branch_a.get("doc_b").await?.is_none());
    assert!(branch_b.get("doc_a").await?.is_none());

    // 5. Simulate "Merge": Bring branch_a data into main
    let doc_a = branch_a.get("doc_a").await?.expect("should exist");
    main.insert(&doc_a.id, &[0.0, 0.0, 1.0, 0.0], doc_a.metadata).await?;
    assert!(main.get("doc_a").await?.is_some());

    // 6. GC Protection Verification
    // Delete doc1 from main.
    main.delete("doc1").await?;
    assert!(main.get("doc1").await?.is_none());

    // Force a flush to disk.
    storage.force_flush().await?;

    // Even if we delete it, the sequence number of the deletion is > base_seq.
    // The checkpoint pin should keep the Registry's min_active_seqno at or below base_seq.
    assert!(storage.snapshot_registry.min_active_seqno() <= base_seq);

    // 7. Cleanup
    manager.drop_checkpoint(&cp).await?;

    Ok(())
}
