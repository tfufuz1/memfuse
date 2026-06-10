use memfuse_cluster::{storage::Store, TypeConfig};
use memfuse_core::{StorageEngine, TxId};
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use openraft::storage::RaftSnapshotBuilder;
use openraft::storage::RaftStateMachine;
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_snapshot_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let tmp1 = TempDir::new()?;
    let tmp2 = TempDir::new()?;

    // 1. Setup Source Store
    let config1 = LsmConfig {
        path: tmp1.path().to_path_buf(),
        ..Default::default()
    };
    let lsm1 = Arc::new(LsmStorage::new(config1).await?);
    let mut store1 = Store::new(Arc::clone(&lsm1));

    // 2. Insert data into Source
    let tx_id = TxId::new(1);
    lsm1.put(tx_id, b"raft-key-1", b"raft-value-1").await?;
    lsm1.put(tx_id, b"raft-key-2", b"raft-value-2").await?;
    lsm1.commit(tx_id).await?;

    // Simulate Raft applying entries to update last_applied_log
    let log_id = openraft::LogId::new(openraft::CommittedLeaderId::new(1, 1), 10);
    let entry = openraft::Entry::<TypeConfig> {
        log_id,
        payload: openraft::EntryPayload::Blank,
    };
    store1.apply(vec![entry]).await?;

    // 3. Build Snapshot
    let mut builder = store1.get_snapshot_builder().await;
    let snapshot = builder.build_snapshot().await?;
    let meta = snapshot.meta.clone();

    // 4. Setup Target Store
    let config2 = LsmConfig {
        path: tmp2.path().to_path_buf(),
        ..Default::default()
    };
    let lsm2 = Arc::new(LsmStorage::new(config2).await?);
    let mut store2 = Store::new(lsm2);

    // 5. Install Snapshot on Target
    store2.install_snapshot(&meta, snapshot.snapshot).await?;

    // 6. Verify data on Target
    let val1 = store2.lsm.get(b"raft-key-1").await?;
    let val2 = store2.lsm.get(b"raft-key-2").await?;

    assert_eq!(val1, Some(b"raft-value-1".to_vec()));
    assert_eq!(val2, Some(b"raft-value-2".to_vec()));

    // Verify metadata on Target
    let (last_applied, _) = store2.applied_state().await?;
    assert_eq!(last_applied, Some(log_id));

    Ok(())
}

#[tokio::test]
async fn test_applied_state_initial() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = TempDir::new()?;
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        ..Default::default()
    };
    let lsm = Arc::new(LsmStorage::new(config).await?);
    let mut store = Store::new(lsm);

    let (last_applied, membership) = store.applied_state().await?;
    assert!(last_applied.is_none());
    assert!(membership.nodes().next().is_none());

    Ok(())
}
