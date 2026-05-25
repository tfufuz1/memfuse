// AGENT:12 DATE:2026-05-25 STATUS:READY
// ANCHOR:INTEGRATION:CORE-SNAPSHOT-001 — SnapshotRegistry visibility and min_active_seqno.

use memfuse_core::SnapshotRegistry;
use std::sync::Arc;
use tokio::task;

#[tokio::test]
async fn test_snapshot_registry_visibility_concurrent() {
    let registry = Arc::new(SnapshotRegistry::new());

    assert_eq!(registry.min_active_seqno(), u64::MAX);

    // 1. Multiple threads register snapshots
    let r1 = registry.clone();
    let h1 = task::spawn(async move {
        let _g = r1.register(100);
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        _g.seq_no() // return to keep it alive until here
    });

    let r2 = registry.clone();
    let h2 = task::spawn(async move {
        let _g = r2.register(200);
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        _g.seq_no()
    });

    // Give them a moment to register
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    assert_eq!(registry.min_active_seqno(), 100);

    h1.await.unwrap();
    // After h1 finishes, guard is dropped, min should be 200
    assert_eq!(registry.min_active_seqno(), 200);

    h2.await.unwrap();
    // After h2 finishes, min should be MAX
    assert_eq!(registry.min_active_seqno(), u64::MAX);
}

#[tokio::test]
async fn test_snapshot_pin_unpin_integration() {
    let registry = Arc::new(SnapshotRegistry::new());

    registry.pin(500);
    assert_eq!(registry.min_active_seqno(), 500);

    let _g = registry.register(1000);
    assert_eq!(registry.min_active_seqno(), 500);

    registry.unpin(500);
    assert_eq!(registry.min_active_seqno(), 1000);

    drop(_g);
    assert_eq!(registry.min_active_seqno(), u64::MAX);
}

#[tokio::test]
async fn test_many_concurrent_snapshots() {
    let registry = Arc::new(SnapshotRegistry::new());
    let num_snapshots = 100;
    let mut handles = Vec::new();

    for i in 0..num_snapshots {
        let r = registry.clone();
        handles.push(task::spawn(async move {
            let seq = (i + 1) * 10;
            let _g = r.register(seq as u64);
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }));
    }

    // Wait a bit to ensure all tasks have started and registered their snapshots
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // min should be 10 (from i=0)
    assert_eq!(registry.min_active_seqno(), 10);

    for h in handles {
        h.await.unwrap();
    }

    assert_eq!(registry.min_active_seqno(), u64::MAX);
}
