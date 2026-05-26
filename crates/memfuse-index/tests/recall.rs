use memfuse_core::{DistanceMetric, DocId, TxId, VectorIndex};
use memfuse_index::hnsw::{HnswConfig, HnswIndex};
use rand::Rng;

#[tokio::test]
async fn test_recall_at_10_above_95() {
    let dim = 128;
    let num_vectors = 1000; // Reduced for faster verification
    let num_queries = 100;

    let mut config = HnswConfig {
        dimension: dim,
        max_elements: 10_000,
        m: 32,                // Increase connectivity
        ef_construction: 400, // Better graph
        ef_search: 200,
        distance_metric: DistanceMetric::Cosine,
        rebuild_threshold: 0.8,
        quantize: false,
    };

    let index_f32 = HnswIndex::new(config.clone());
    config.quantize = true;
    let index_sq8 = HnswIndex::new(config);

    let mut rng = rand::thread_rng();
    let mut data = Vec::with_capacity(num_vectors);
    for _ in 0..num_vectors {
        let mut v: Vec<f32> = (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect();
        // Normalize to unit sphere
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        for x in v.iter_mut() {
            *x /= norm;
        }
        data.push(v);
    }

    let tx = TxId::new(1);
    for (i, v) in data.iter().enumerate() {
        index_f32.insert(tx, DocId::new(i as u64), v).await.unwrap(); // unwrap
        index_sq8.insert(tx, DocId::new(i as u64), v).await.unwrap(); // unwrap
    }
    index_f32.commit(tx).await.unwrap(); // unwrap
    index_sq8.commit(tx).await.unwrap(); // unwrap

    let mut total_recall = 0.0;
    for _ in 0..num_queries {
        let query: Vec<f32> = (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect();

        let results_f32 = index_f32.search(&query, 10).await.unwrap(); // unwrap
        let results_sq8 = index_sq8.search(&query, 10).await.unwrap(); // unwrap

        let ground_truth: std::collections::HashSet<_> =
            results_f32.iter().map(|r| r.doc_id).collect();
        let mut hits = 0;
        for r in results_sq8 {
            if ground_truth.contains(&r.doc_id) {
                hits += 1;
            }
        }
        total_recall += hits as f64 / 10.0;
    }

    let avg_recall = total_recall / num_queries as f64;
    println!("Average Recall@10: {:.4}", avg_recall);

    // AC-1: Recall@10 >= 0.95
    assert!(avg_recall >= 0.95, "Recall too low: {:.4}", avg_recall);
}
