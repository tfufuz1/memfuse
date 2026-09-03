//! Integration test for DiskANN Vamana graph build and Recall@10 accuracy (BEFUND 1).
//! Verifies that DiskANN achieves Recall@10 >= 0.95 on synthetic dataset.
//! Ground-truth top-10 neighbors are computed via an independent brute-force reference implementation.

use memfuse_core::{DistanceMetric, DocId, VectorIndex};
use memfuse_index::diskann::{DiskAnnConfig, DiskAnnIndex};
use rand::Rng;
use std::collections::HashSet;

/// Independent brute-force Cosine distance calculation (anti-mirroring requirement).
/// This implementation is strictly separate from `memfuse_index::distance::compute_distance`.
fn independent_brute_force_cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0f32;
    let mut norm_a_sq = 0.0f32;
    let mut norm_b_sq = 0.0f32;
    for (&x, &y) in a.iter().zip(b.iter()) {
        dot += x * y;
        norm_a_sq += x * x;
        norm_b_sq += y * y;
    }
    if norm_a_sq <= 0.0 || norm_b_sq <= 0.0 {
        1.0
    } else {
        let sim = dot / (norm_a_sq.sqrt() * norm_b_sq.sqrt());
        (1.0 - sim).max(0.0)
    }
}

/// Computes independent top-k ground truth doc_ids using brute-force exact search.
fn independent_brute_force_knn(
    query: &[f32],
    vectors: &[Vec<f32>],
    ids: &[DocId],
    k: usize,
) -> HashSet<DocId> {
    let mut scored: Vec<(DocId, f32)> = vectors
        .iter()
        .zip(ids.iter())
        .map(|(v, &id)| {
            let dist = independent_brute_force_cosine_distance(query, v);
            (id, dist)
        })
        .collect();

    scored.sort_by(|a, b| a.1.total_cmp(&b.1));
    scored.into_iter().take(k).map(|(id, _)| id).collect()
}

#[tokio::test]
async fn test_diskann_recall_at_10_above_95() {
    let dim = 32;
    let num_vectors = 10_000;
    let num_queries = 50;
    let k = 10;

    let temp_dir = tempfile::tempdir().expect("tempdir");
    let index_path = temp_dir.path().join("diskann_recall.idx");

    let config = DiskAnnConfig {
        index_path,
        dimension: dim,
        max_degree: 32,
        beam_width: 64,
        sector_size: 4096,
        distance_metric: DistanceMetric::Cosine,
        quantize: false,
        ..DiskAnnConfig::default()
    };

    let index = DiskAnnIndex::try_new(config).expect("valid DiskAnnConfig");

    let mut rng = rand::thread_rng();

    // 1. Generate 10,000 synthetic random vectors
    let mut vectors = Vec::with_capacity(num_vectors);
    let mut ids = Vec::with_capacity(num_vectors);
    for i in 0..num_vectors {
        let mut v: Vec<f32> = (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect();
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in v.iter_mut() {
                *x /= norm;
            }
        }
        vectors.push(v);
        ids.push(DocId::from(i as u64 + 1));
    }

    // 2. Generate 50 ground-truth query vectors
    let mut queries = Vec::with_capacity(num_queries);
    for _ in 0..num_queries {
        let mut q: Vec<f32> = (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect();
        let norm: f32 = q.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in q.iter_mut() {
                *x /= norm;
            }
        }
        queries.push(q);
    }

    // 3. Compute independent ground-truth kNN for each query
    let ground_truths: Vec<HashSet<DocId>> = queries
        .iter()
        .map(|q| independent_brute_force_knn(q, &vectors, &ids, k))
        .collect();

    // 4. Build the DiskANN index (uses in-memory graph passes)
    index
        .build(&vectors, &ids)
        .await
        .expect("DiskANN build failed");

    // 5. Execute 50 queries and compute Recall@10
    let mut total_recall = 0.0;
    for (q, gt) in queries.iter().zip(ground_truths.iter()) {
        let search_results = index.search(q, k).await.expect("Search failed");
        let hits = search_results
            .iter()
            .filter(|r| gt.contains(&r.doc_id))
            .count();
        total_recall += hits as f64 / k as f64;
    }

    let avg_recall = total_recall / num_queries as f64;
    println!("DiskANN Recall@10: {:.4}", avg_recall);

    // AC: Recall@10 >= 0.95
    assert!(
        avg_recall >= 0.95,
        "DiskANN Recall@10 too low: {:.4} (expected >= 0.95)",
        avg_recall
    );
}
