// ANCHOR:INTEGRATION:CORE-001 STATUS:READY AGENT:12
use memfuse_core::{DocId, IndexOp, SnapshotRegistry, TxBuffer, TxId};
use std::sync::Arc;

#[test]
fn test_core_tx_buffer_lifecycle() {
    let buffer = TxBuffer::<Vec<u8>>::new();
    let tx = TxId::new(42);

    buffer.begin(tx);
    buffer.stage(
        tx,
        IndexOp::Insert {
            doc_id: DocId::new(1),
            data: vec![1, 2, 3],
        },
    );

    assert!(buffer.has_tx(tx));
    let ops = buffer.drain(tx);
    assert_eq!(ops.len(), 1);
    assert!(!buffer.has_tx(tx));
}

#[test]
fn test_core_snapshot_visibility() {
    let registry = Arc::new(SnapshotRegistry::new());

    let snap1 = registry.register(10);
    assert_eq!(snap1.seq_no(), 10);
    assert_eq!(registry.min_active_seqno(), 10);

    let snap2 = registry.register(20);
    assert_eq!(registry.min_active_seqno(), 10);

    drop(snap1);
    assert_eq!(registry.min_active_seqno(), 20);

    drop(snap2);
    assert_eq!(registry.min_active_seqno(), u64::MAX);
}
