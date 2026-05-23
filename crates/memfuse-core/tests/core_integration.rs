use memfuse_core::{DocId, EntityId, IndexOp, MemFuseError, SnapshotRegistry, TxBuffer, TxId};
use std::sync::Arc;

// ANCHOR:INTEGRATION:CORE-001 STATUS:DONE AGENT:12 DATE:2026-05-23
// Integration test for core components like TxBuffer and SnapshotRegistry.
#[test]
fn test_core_tx_buffer_isolation() {
    let buffer = TxBuffer::<Vec<u8>>::new();
    let tx1 = TxId::new(1);
    let tx2 = TxId::new(2);

    buffer.stage(tx1, IndexOp::Insert { doc_id: DocId::new(1), data: b"val1".to_vec() });
    buffer.stage(tx2, IndexOp::Insert { doc_id: DocId::new(1), data: b"val2".to_vec() });

    // Verify isolation in buffer
    let ops1 = buffer.drain(tx1);
    let ops2 = buffer.drain(tx2);

    assert_eq!(ops1.len(), 1);
    assert_eq!(ops2.len(), 1);

    match &ops1[0] {
        IndexOp::Insert { doc_id, data } => {
            assert_eq!(doc_id.inner(), 1);
            assert_eq!(data.as_slice(), b"val1");
        }
        _ => panic!("Expected Insert op"),
    }
}

#[test]
fn test_snapshot_registry_visibility() {
    let registry = Arc::new(SnapshotRegistry::new());

    // Initial state
    assert_eq!(registry.min_active_seqno(), u64::MAX);

    // Register some snapshots
    let s1 = registry.register(10);
    let s2 = registry.register(20);

    assert_eq!(registry.min_active_seqno(), 10);

    // Release oldest
    drop(s1);
    assert_eq!(registry.min_active_seqno(), 20);

    // Release remaining
    drop(s2);
    assert_eq!(registry.min_active_seqno(), u64::MAX);
}

#[test]
fn test_error_conversions() {
    let err = MemFuseError::Internal("test".to_string());
    let msg = format!("{}", err);
    assert!(msg.contains("Internal"));
    assert!(msg.contains("test"));
}

#[test]
fn test_type_identities() {
    let d1 = DocId::new(1);
    let d2 = DocId::new(1);
    let d3 = DocId::new(2);

    assert_eq!(d1, d2);
    assert_ne!(d1, d3);

    let e1 = EntityId::new(1);
    assert_eq!(e1.inner(), 1);
}
