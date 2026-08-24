use memfuse_core::{DocId, TxId, VectorIndex};
use memfuse_index::hnsw::{HnswConfig, HnswIndex};
use rand::{Rng, SeedableRng};
use std::sync::Arc;

#[tokio::test]
async fn test_quantizer_recalibration() {
    let config = HnswConfig {
        dimension: 4,
        max_elements: 1000,
        m: 16,
        ef_construction: 100,
        ef_search: 50,
        rebuild_threshold: 0.8,
        distance_metric: memfuse_core::DistanceMetric::Euclidean,
        quantize: true,
        quantizer_recalibration_sample_size: 1000,
        ..Default::default()
    };
    let index = Arc::new(HnswIndex::new(config));

    // Phase 1: Lazy training on initial 256 vectors with small values [0.0, 1.0]
    let mut rng = rand::rngs::StdRng::seed_from_u64(42);
    for i in 0..256 {
        let v = vec![
            rng.gen_range(0.0..1.0),
            rng.gen_range(0.0..1.0),
            rng.gen_range(0.0..1.0),
            rng.gen_range(0.0..1.0),
        ];
        index.insert(TxId(1), DocId(i as u64), &v).await.unwrap();
    }
    index.commit(TxId(1)).await.unwrap();

    // Verify initial quantizer state
    let q_before = {
        let guard = index.quantizer.read();
        guard.as_ref().unwrap().clone()
    };
    for &m in &q_before.maxes {
        assert!(m <= 1.05);
    }

    // Phase 2: Insert out-of-distribution vectors [100.0, 200.0]
    let ood_vector = vec![150.0, 150.0, 150.0, 150.0];
    index.insert(TxId(2), DocId(256), &ood_vector).await.unwrap();
    index.commit(TxId(2)).await.unwrap();
    
    // Trigger rebuild to recalibrate
    index.rebuild().await.unwrap();

    // Verify new quantizer state
    let q_after = {
        let guard = index.quantizer.read();
        guard.as_ref().unwrap().clone()
    };
    // The new quantizer maxes should adapt to include 150.0
    assert!(q_after.maxes[0] > 100.0);

    // After rebuild: the out-of-distribution vector is correctly quantized (no clamping)
    let new_ood_vector = vec![160.0, 160.0, 160.0, 160.0];
    index.insert(TxId(3), DocId(257), &new_ood_vector).await.unwrap();
    index.commit(TxId(3)).await.unwrap();

    let search_res = index.search(&new_ood_vector, 1).await.unwrap();
    assert_eq!(search_res[0].doc_id, DocId(257));
}
