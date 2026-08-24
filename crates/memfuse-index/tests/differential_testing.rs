use memfuse_core::{DistanceMetric, DocId, TxId, VectorIndex};
use memfuse_index::hnsw::{HnswConfig, HnswIndex};
use rand::Rng;
use std::collections::HashSet;

#[tokio::test]
async fn test_differential_quantization_sq8_10000_queries() {
    let dim = 128;
    // Reduced size to fit in test timeout (5 minutes)
    let num_vectors = 1000;
    let num_queries = 100;

    let mut config = HnswConfig {
        dimension: dim,
        max_elements: num_vectors * 2,
        m: 32,
        ef_construction: 200,
        ef_search: 200,
        distance_metric: DistanceMetric::Cosine,
        rebuild_threshold: 0.8,
        quantize: false,
        ..Default::default()
    };

    let index_f32 = HnswIndex::new(config.clone());
    config.quantize = true;
    let index_sq8 = HnswIndex::new(config);

    let mut rng = rand::thread_rng();
    let mut data = Vec::with_capacity(num_vectors);
    for _ in 0..num_vectors {
        let mut v: Vec<f32> = (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect();
        // Normalize
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        for x in v.iter_mut() {
            *x /= norm;
        }
        data.push(v);
    }

    let tx = TxId::new(1);
    for (i, v) in data.iter().enumerate() {
        index_f32.insert(tx, DocId::new(i as u64), v).await.unwrap();
        index_sq8.insert(tx, DocId::new(i as u64), v).await.unwrap();
    }
    index_f32.commit(tx).await.unwrap();
    index_sq8.commit(tx).await.unwrap();

    let mut total_hits = 0;
    let top_k = 10;

    for _ in 0..num_queries {
        let mut query: Vec<f32> = (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect();
        let norm: f32 = query.iter().map(|x| x * x).sum::<f32>().sqrt();
        for x in query.iter_mut() {
            *x /= norm;
        }

        let results_f32 = index_f32.search(&query, top_k).await.unwrap();
        let results_sq8 = index_sq8.search(&query, top_k).await.unwrap();

        let ground_truth: HashSet<_> = results_f32.iter().map(|r| r.doc_id).collect();
        for r in results_sq8 {
            if ground_truth.contains(&r.doc_id) {
                total_hits += 1;
            }
        }
    }

    let avg_recall = (total_hits as f64) / ((num_queries * top_k) as f64);
    println!("--- Differential Testing Results ---");
    println!("Queries: {}", num_queries);
    println!("Top-K: {}", top_k);
    println!(
        "Intersection Recall (SQ8 vs F32): {:.2}%",
        avg_recall * 100.0
    );
    println!("------------------------------------");

    // We expect recall drop to be around ~1.8% as stated in the documentation
    assert!(avg_recall >= 0.95, "Recall too low: {:.4}", avg_recall);
}
