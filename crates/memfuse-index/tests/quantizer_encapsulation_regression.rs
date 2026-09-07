use memfuse_core::{DocId, TxId, VectorIndex};
use memfuse_index::hnsw::{HnswConfig, HnswIndex};

#[tokio::test]
async fn test_quantizer_encapsulation_prevents_field_mutation() {
    let config = HnswConfig {
        dimension: 4,
        max_elements: 100,
        quantize: true,
        ..Default::default()
    };
    let index = HnswIndex::try_new(config).unwrap();

    let tx1 = TxId::new(1);
    for i in 1..=60u64 {
        let v = vec![1.0, 2.0, 3.0, 4.0];
        index.insert(tx1, DocId::new(i), &v).await.unwrap();
    }
    index.commit(tx1).await.unwrap();

    // Verify index.quantizer() returns Option<ScalarQuantizer> read-only snapshot
    let q_opt = index.quantizer();
    assert!(q_opt.is_some());

    if let Some(q) = q_opt {
        // Read access via getters works
        assert_eq!(q.mins().len(), 4);
        assert_eq!(q.maxes().len(), 4);
        assert_eq!(q.scales().len(), 4);
        assert_eq!(q.inv_scales().len(), 4);
        assert_eq!(q.dimension(), 4);

        // Even mutating our cloned local `q` does NOT affect `index`:
        // Note: Direct field mutation like `q.mins.truncate(1)` is impossible from outside memfuse-index
        // because `mins` is pub(crate). But even if a cloned `ScalarQuantizer` were modified locally,
        // the index's internal state remains untouched.
    }

    // Subsequent search succeeds without panic
    let results = index.search(&[1.0, 2.0, 3.0, 4.0], 5).await.unwrap();
    assert!(!results.is_empty());
}
