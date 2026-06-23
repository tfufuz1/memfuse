use memfuse_core::{
    snapshot::SnapshotRegistry,
    traits::DistanceCalculator,
    tx_buffer::{IndexOp, TxBuffer},
    types::{DocId, TxId},
};
use std::sync::Arc;
use std::time::Duration;

#[test]
fn test_integration_tx_buffer_and_snapshots() {
    // Phase 1: Set up TX Buffer and Snapshot Registry
    let buffer = TxBuffer::<String>::new_with_config(8, Duration::from_secs(60));
    let registry = Arc::new(SnapshotRegistry::new());

    // Transaction 1
    let tx1 = TxId::new(100);
    buffer.begin(tx1);
    buffer.stage(
        tx1,
        IndexOp::Insert {
            doc_id: DocId::new(1),
            data: "first_doc".to_string(),
        },
    );

    // Take a snapshot
    let guard = registry.register(101);

    // Transaction 2
    let tx2 = TxId::new(102);
    buffer.begin(tx2);
    buffer.stage(
        tx2,
        IndexOp::Insert {
            doc_id: DocId::new(2),
            data: "second_doc".to_string(),
        },
    );

    // Assert isolated tx
    assert_eq!(buffer.len(), 2);
    let drained_tx1 = buffer.drain(tx1);
    assert_eq!(drained_tx1.len(), 1);
    assert_eq!(drained_tx1[0].doc_id(), DocId::new(1));

    // Snapshot guard is still holding 101 as minimum sequence number
    assert_eq!(registry.min_active_seqno(), 101);
    drop(guard);
    assert_eq!(registry.min_active_seqno(), u64::MAX);

    // Clear tx2
    buffer.discard(tx2);
    assert!(buffer.is_empty());
}

#[test]
fn test_domain_metrics_integration() {
    use memfuse_core::types::DistanceMetric;
    
    // Testing the integration between the generic DistanceMetric enum and the Calculator Trait
    let dyn_calc: &dyn DistanceCalculator = &DistanceMetric::Cosine;
    
    let a = [1.0, 0.0, 0.0];
    let b = [1.0, 0.0, 0.0];
    
    // Exact match in Cosine means distance is 0.0 ideally
    let dist = dyn_calc.compute_f32(&a, &b).unwrap();
    assert!(dist < 0.0001); // floating point tolerance
}
