use memfuse_core::{
    snapshot::SnapshotRegistry,
    tx_buffer::{IndexOp, TxBuffer},
    types::{DocId, DistanceMetric, FusionWeights, ResourceBudget, ResourceTracker, TxId},
};
use std::sync::Arc;
use std::time::Duration;

#[test]
fn test_fusion_weights_nan_and_inf_prevention() {
    // Expose the NaN vulnerability: FusionWeights should not allow NaN weights.
    let res_nan = FusionWeights::new(f32::NAN, 0.5, 0.5, 0.0);
    assert!(res_nan.is_err(), "FusionWeights should reject NaN values");

    let res_neg_nan = FusionWeights::new(-f32::NAN, 0.5, 0.5, 0.0);
    assert!(res_neg_nan.is_err(), "FusionWeights should reject negative NaN values");

    // Expose the Inf vulnerability: FusionWeights should not allow Inf weights.
    let res_inf = FusionWeights::new(f32::INFINITY, 0.5, 0.5, 0.0);
    assert!(res_inf.is_err(), "FusionWeights should reject Infinity values");
}

#[test]
fn test_distance_metric_nan_inf_prevention() {
    let metric = DistanceMetric::Cosine;

    // Cosine with NaN in first vector
    let a = [f32::NAN, 0.0];
    let b = [1.0, 1.0];
    let res = metric.compute(&a, &b);
    assert!(res.is_err(), "Distance computation should fail if inputs contain NaN");

    // Cosine with Inf in first vector
    let a = [f32::INFINITY, 0.0];
    let b = [1.0, 1.0];
    let res = metric.compute(&a, &b);
    assert!(res.is_err(), "Distance computation should fail if inputs contain Infinity");

    // Euclidean with NaN
    let metric_e = DistanceMetric::Euclidean;
    let a = [0.0, f32::NAN];
    let b = [1.0, 1.0];
    let res = metric_e.compute(&a, &b);
    assert!(res.is_err(), "Euclidean distance computation should fail if inputs contain NaN");

    // DotProduct with NaN
    let metric_dp = DistanceMetric::DotProduct;
    let a = [f32::NAN, 0.0];
    let b = [1.0, 1.0];
    let res = metric_dp.compute(&a, &b);
    assert!(res.is_err(), "DotProduct distance computation should fail if inputs contain NaN");
}

#[test]
fn test_snapshot_registry_robustness_and_concurrency() {
    let registry = Arc::new(SnapshotRegistry::new());
    let thread_count = 10;
    let iterations = 1000;
    let mut handles = Vec::new();

    for i in 0..thread_count {
        let r = registry.clone();
        handles.push(std::thread::spawn(move || {
            for j in 0..iterations {
                let seq = (i * iterations + j) as u64;
                let guard = r.register(seq);
                assert!(r.min_active_seqno() <= seq);
                drop(guard);
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    assert_eq!(registry.min_active_seqno(), u64::MAX);
}

#[test]
fn test_resource_tracker_edge_cases() {
    let budget = ResourceBudget { memory_limit: 1000 };
    let tracker = ResourceTracker::new(budget);

    // consume extreme values
    assert!(tracker.consume_memory(u64::MAX).is_err());
    assert!(tracker.consume_memory(1001).is_err());

    // release extreme values
    tracker.release_memory(u64::MAX);
    assert_eq!(tracker.memory_used(), 0);
}

#[test]
fn test_tx_buffer_orphan_reaper_concurrency() {
    let buffer = Arc::new(TxBuffer::<String>::new_with_config(8, Duration::from_millis(5)));
    let thread_count = 5;
    let iterations = 100;
    let mut handles = Vec::new();

    // Spawn writers staging transactions
    for t in 0..thread_count {
        let b = buffer.clone();
        handles.push(std::thread::spawn(move || {
            for i in 0..iterations {
                let tx = TxId::new((t * iterations + i) as u64);
                b.begin(tx);
                b.stage(tx, IndexOp::Insert { doc_id: DocId::new(i as u64), data: "test".to_string() });
                std::thread::sleep(Duration::from_micros(100));
            }
        }));
    }

    // Spawn reaper
    let b_reap = buffer.clone();
    let reaper_handle = std::thread::spawn(move || {
        for _ in 0..50 {
            let _reaped = b_reap.reap_orphans();
            std::thread::sleep(Duration::from_millis(2));
        }
    });

    for h in handles {
        h.join().unwrap();
    }
    reaper_handle.join().unwrap();

    // After letting it rest and reaping one last time, it should be clean
    std::thread::sleep(Duration::from_millis(10));
    buffer.reap_orphans();
    assert!(buffer.is_empty(), "Orphan reaper should eventually clean everything");
}
