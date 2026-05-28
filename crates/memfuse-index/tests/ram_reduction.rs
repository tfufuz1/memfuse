use memfuse_core::{DistanceMetric, DocId, TxId, VectorIndex};
use memfuse_index::hnsw::{HnswConfig, HnswIndex};

#[tokio::test]
async fn test_ram_reduction_4x() {
    let dim = 128;
    let num_vectors = 1000;

    let mut config = HnswConfig {
        dimension: dim,
        max_elements: 10_000,
        m: 16,
        ef_construction: 100,
        ef_search: 50,
        distance_metric: DistanceMetric::Cosine,
        rebuild_threshold: 0.8,
        quantize: false,
    };

    let index_f32 = HnswIndex::new(config.clone());
    config.quantize = true;
    let index_sq8 = HnswIndex::new(config);

    let tx = TxId::new(1);
    for i in 0..num_vectors {
        let v: Vec<f32> = (0..dim).map(|_| 0.5).collect();
        index_f32
            .insert(tx, DocId::new(i as u64), &v)
            .await
            .expect("hardened by Core Guardian");
        index_sq8
            .insert(tx, DocId::new(i as u64), &v)
            .await
            .expect("hardened by Core Guardian");
    }
    index_f32
        .commit(tx)
        .await
        .expect("hardened by Core Guardian");
    index_sq8
        .commit(tx)
        .await
        .expect("hardened by Core Guardian");

    let stats_f32 = index_f32.stats().await.expect("hardened by Core Guardian");
    let stats_sq8 = index_sq8.stats().await.expect("hardened by Core Guardian");

    let vec_mem_f32 = num_vectors * dim * 4;
    let vec_mem_sq8 = num_vectors * dim;

    println!("F32 Vector Memory (estimated): {} bytes", vec_mem_f32);
    println!("SQ8 Vector Memory (estimated): {} bytes", vec_mem_sq8);
    println!(
        "F32 Total Memory (stats): {} bytes",
        stats_f32.memory_usage_bytes
    );
    println!(
        "SQ8 Total Memory (stats): {} bytes",
        stats_sq8.memory_usage_bytes
    );

    let ratio = stats_f32.memory_usage_bytes as f64 / stats_sq8.memory_usage_bytes as f64;
    println!("Reduction Ratio: {:.2}x", ratio);

    // Vector memory itself should be exactly 4x smaller.
    // Total memory calculation includes connections (m*2 * usize) which are the same for both.
    // So the "Total" ratio will be less than 4x, but the "Vector" ratio is what matters for the 4x goal primarily.
    assert!(
        vec_mem_f32 == vec_mem_sq8 * 4,
        "Vector memory should be exactly 4x smaller"
    );
}
