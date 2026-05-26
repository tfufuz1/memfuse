// ANCHOR:INTEGRATION:CORE-002 STATUS:DONE AGENT:12 DATE:2026-06-21
//! Integration tests for SnapshotRegistry visibility.

use memfuse_core::SnapshotRegistry;
use std::sync::Arc;

#[test]
fn test_snapshot_registry_visibility_logic() {
    let registry = Arc::new(SnapshotRegistry::new());

    // Initially u64::MAX
    assert_eq!(registry.min_active_seqno(), u64::MAX);

    // Register oldest snapshot
    let g1 = registry.register(100);
    assert_eq!(registry.min_active_seqno(), 100);

    // Register newer snapshot
    let g2 = registry.register(200);
    assert_eq!(registry.min_active_seqno(), 100, "Min should still be 100");

    // Register even older snapshot
    let g3 = registry.register(50);
    assert_eq!(registry.min_active_seqno(), 50, "Min should drop to 50");

    // Drop g3 (50)
    drop(g3);
    assert_eq!(registry.min_active_seqno(), 100, "Min should return to 100");

    // Drop g1 (100)
    drop(g1);
    assert_eq!(registry.min_active_seqno(), 200, "Min should move to 200");

    // Drop g2 (200)
    drop(g2);
    assert_eq!(registry.min_active_seqno(), u64::MAX, "Min should be MAX again");
}

#[test]
fn test_manual_pinning_prevents_gc() {
    let registry = Arc::new(SnapshotRegistry::new());

    registry.pin(500);
    assert_eq!(registry.min_active_seqno(), 500);

    let _g = registry.register(1000);
    assert_eq!(registry.min_active_seqno(), 500);

    registry.unpin(500);
    assert_eq!(registry.min_active_seqno(), 1000);
}

#[test]
fn test_snapshot_ref_counting() {
    let registry = Arc::new(SnapshotRegistry::new());

    let _g1 = registry.register(100);
    let _g2 = registry.register(100);

    assert_eq!(registry.min_active_seqno(), 100);

    drop(_g1);
    assert_eq!(registry.min_active_seqno(), 100, "Min should still be 100 because _g2 is active");

    drop(_g2);
    assert_eq!(registry.min_active_seqno(), u64::MAX);
}
