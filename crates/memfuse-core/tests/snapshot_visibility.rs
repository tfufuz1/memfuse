use memfuse_core::SnapshotRegistry;
use std::sync::Arc;
#[tokio::test]
async fn test_snapshot_registry() {
    let registry = Arc::new(SnapshotRegistry::new());
    assert_eq!(registry.min_active_seqno(), u64::MAX);
    let _g = registry.register(100);
    assert_eq!(registry.min_active_seqno(), 100);
}
